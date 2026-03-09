#![no_std]
#![no_main]

mod imu;

use crate::imu::{Imu, ImuRessources, Sample};
use core::fmt::Write;
use defmt::info;
use embassy_executor::{InterruptExecutor, Spawner};
use embassy_futures::yield_now;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::interrupt::Priority;
use embassy_nrf::peripherals::P0_26;
use embassy_nrf::spim::Spim;
use embassy_nrf::twim::{self};
use embassy_nrf::{Peri, bind_interrupts, interrupt, spim};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::Delay;
use embassy_time::{Instant, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::SdCard;
use embedded_sdmmc::TimeSource;
use embedded_sdmmc::Timestamp;
use embedded_sdmmc::VolumeIdx;
use embedded_sdmmc::VolumeManager;
use heapless::{String, Vec};
use {defmt_rtt as _, panic_probe as _};

static EXECUTOR_RT: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn EGU1_SWI1() {
    unsafe { EXECUTOR_RT.on_interrupt() }
}

bind_interrupts!(struct Irqs {
    TWISPI0 => twim::InterruptHandler<embassy_nrf::peripherals::TWISPI0>;
    TWISPI1 => spim::InterruptHandler<embassy_nrf::peripherals::TWISPI1>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_nrf::config::Config::default();
    // Use LXFO to prevent time drift for gyro logs and HFXO for reliable SPI/I2C and DMA
    config.lfclk_source = embassy_nrf::config::LfclkSource::ExternalXtal;
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;

    let p = embassy_nrf::init(config);

    interrupt::EGU1_SWI1.set_priority(Priority::P1);
    let rt_spawner = EXECUTOR_RT.start(interrupt::EGU1_SWI1);

    let resources = ImuRessources::new(p.P1_08, p.P0_11, p.GPIOTE_CH0, p.TWISPI0, p.P0_07, p.P0_27);

    info!("Spawning tasks");
    let _ = rt_spawner.spawn(sample_task(p.P0_26, resources)).unwrap();
    IMU_READY.wait().await;

    let cs = Output::new(p.P0_29, Level::High, OutputDrive::Standard);

    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;

    let spi_bus = Spim::new(p.TWISPI1, Irqs, p.P1_13, p.P1_14, p.P1_15, spi_config);

    let spi_device = ExclusiveDevice::new(spi_bus, cs, Delay).unwrap();

    let _ = spawner.spawn(do_sd_card(spi_device)).unwrap();
    info!("Spawned tasks");

    // Softhalt
    loop {
        Timer::after_secs(100).await;
    }
}

static SAMPLES_CHANNEL: Channel<CriticalSectionRawMutex, Sample, 1024> = Channel::new();
static COMPLETE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static IMU_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
async fn sample_task(power_led: Peri<'static, P0_26>, resources: ImuRessources) {
    let mut led = Output::new(power_led, Level::High, OutputDrive::Standard);

    info!("Initializing IMU");
    let mut imu = Imu::init(resources).await;
    info!("IMU ready");
    IMU_READY.signal(());
    let sender = SAMPLES_CHANNEL.sender();

    let start = Instant::now();
    info!("Started sampling");
    let mut samples = Vec::new();
    loop {
        imu.data_interrupt().await;

        led.set_low();

        samples.clear();
        imu.read_samples(&mut samples).await;
        for sample in &samples {
            sender.send(*sample).await;
        }

        led.set_high();
        if start.elapsed().as_secs() >= 5 {
            break;
        }
    }
    imu.poweroff();
    COMPLETE.signal(());
    info!("Completed sampling");
}

struct DummyClock;

impl TimeSource for DummyClock {
    fn get_timestamp(&self) -> Timestamp {
        // Returns a dummy date: Jan 1, 2024, 00:00:00
        Timestamp::from_calendar(2026, 3, 8, 0, 0, 0).unwrap()
    }
}

#[embassy_executor::task]
async fn do_sd_card(spi_device: ExclusiveDevice<Spim<'static>, Output<'static>, Delay>) {
    let sdcard = SdCard::new(spi_device, Delay);

    let s = sdcard.num_bytes().unwrap();
    info!("SD card is {} bytes", s);

    let volume_mgr = VolumeManager::new(sdcard, DummyClock);
    let volume0 = volume_mgr.open_volume(VolumeIdx(0)).unwrap();
    let root_dir = volume0.open_root_dir().unwrap();

    let my_file = root_dir
        .open_file_in_dir("TEST.TXT", embedded_sdmmc::Mode::ReadWriteCreateOrTruncate)
        .unwrap();

    let r = SAMPLES_CHANNEL.receiver();
    loop {
        if r.is_empty() && COMPLETE.signaled() {
            break;
        }
        let sample = r.receive().await;
        // TODO: Bulk write ops here
        let mut line = String::<128>::new();
        line.clear();
        write!(
            line,
            "{},{},{},{},{},{},{}\n",
            Instant::now().as_micros(), //TODO: Use accurate systemd timing source here
            sample.g_x,
            sample.g_y,
            sample.g_z,
            sample.a_x,
            sample.a_y,
            sample.a_z
        )
        .unwrap();
        my_file.write(line.as_bytes()).unwrap();
        yield_now().await;
    }

    my_file.close().unwrap();
    root_dir.close().unwrap();
    info!("Completed writing");
}
