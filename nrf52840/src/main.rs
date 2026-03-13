#![no_std]
#![no_main]

mod imu;
mod util;
use embassy_nrf::peripherals;
use embassy_nrf::rng;
use crate::util::wait_for_press;
use embassy_sync::pipe::Pipe;
use traccam_common::gyro_format::text::get_header_string;
use crate::imu::SAMPLE_INTERVAL_MICROS;
use crate::imu::{Imu, ImuRessources};
use core::fmt::Write;
use core::ops::Add;
use defmt::info;
use embassy_executor::{InterruptExecutor, Spawner};
use embassy_futures::yield_now;
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pull};
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::interrupt::Priority;
use embassy_nrf::peripherals::{GPIOTE_CH0, P0_26, GPIOTE_CH1};
use embassy_nrf::spim::Spim;
use embassy_nrf::twim::{self};
use embassy_nrf::{Peri, bind_interrupts, interrupt, spim};
use embassy_nrf::gpiote::{InputChannel, InputChannelPolarity};
use embassy_nrf::mode::Async;
use embassy_nrf::rng::Rng;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Delay, Duration};
use embassy_time::{Instant, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::SdCard;
use embedded_sdmmc::TimeSource;
use embedded_sdmmc::Timestamp;
use embedded_sdmmc::VolumeIdx;
use embedded_sdmmc::VolumeManager;
use heapless::{format, String, Vec};
use {defmt_rtt as _, panic_probe as _};

static EXECUTOR_RT: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn EGU1_SWI1() {
    unsafe { EXECUTOR_RT.on_interrupt() }
}

bind_interrupts!(struct Irqs {
    TWISPI0 => twim::InterruptHandler<peripherals::TWISPI0>;
    TWISPI1 => spim::InterruptHandler<peripherals::TWISPI1>;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_nrf::config::Config::default();
    // Use LXFO to prevent time drift for gyro logs and HFXO for reliable SPI/I2C and DMA
    config.lfclk_source = embassy_nrf::config::LfclkSource::ExternalXtal;
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;

    let p = embassy_nrf::init(config);

    // Buttons
    let mut btn_center = InputChannel::new(p.GPIOTE_CH1, p.P0_28, Pull::Up, InputChannelPolarity::HiToLo);

    // RT Scheduler
    interrupt::EGU1_SWI1.set_priority(Priority::P1);
    let rt_spawner = EXECUTOR_RT.start(interrupt::EGU1_SWI1);

    // IMU stuff
    let resources = ImuRessources::new(p.P1_08, p.P0_11, p.GPIOTE_CH0, p.TWISPI0, p.P0_07, p.P0_27);

    // SD stuff
    let cs = Output::new(p.P0_29, Level::High, OutputDrive::Standard);

    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;

    let spi_bus = Spim::new(p.TWISPI1, Irqs, p.P1_13, p.P1_14, p.P1_15, spi_config);

    let spi_device = ExclusiveDevice::new(spi_bus, cs, Delay).unwrap();

    // Misc.
    let rng = Mutex::<CriticalSectionRawMutex, _>::new(Rng::new(p.RNG, Irqs));

    // Spawn tasks
    let _ = rt_spawner.spawn(sample_task(p.P0_26, resources)).unwrap();
    let _ = spawner
        .spawn(do_sd_card(spi_device, rng))
        .unwrap();
    loop {
        wait_for_press::<GPIOTE_CH1>(&mut btn_center).await;
        TOGGLE_RECORDING.signal(());
        info!("Recording started");
        wait_for_press::<GPIOTE_CH1>(&mut btn_center).await;
        info!("Stopping recording");
        TOGGLE_RECORDING.signal(());
        info!("Stopped recording");
        Timer::after_secs(1).await;
    }
}

static SAMPLES: Pipe<CriticalSectionRawMutex, { 1024 * 2 }> = Pipe::new();
static COMPLETE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static IMU_READY: Signal<CriticalSectionRawMutex, Instant> = Signal::new();

static TOGGLE_RECORDING: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
async fn sample_task(power_led: Peri<'static, P0_26>, mut resources: ImuRessources) {
    let mut led = Output::new(power_led, Level::High, OutputDrive::Standard);

    loop {
        // Start recording
        TOGGLE_RECORDING.wait().await;
        let mut imu = Imu::init(resources).await;
        IMU_READY.signal(Instant::now());
        info!("Started sampling");
        resources = loop {
            imu.data_interrupt().await;

            led.set_low();

            imu.read_samples(&SAMPLES).await;

            led.set_high();
            if TOGGLE_RECORDING.signaled() {
                TOGGLE_RECORDING.wait().await;
                info!("Completed sampling");
                COMPLETE.signal(());
                break imu.poweroff();
            }
        };
    }
}

struct DummyClock;

impl TimeSource for DummyClock {
    fn get_timestamp(&self) -> Timestamp {
        // Returns a dummy date: Jan 1, 2024, 00:00:00
        Timestamp::from_calendar(2026, 3, 8, 0, 0, 0).unwrap()
    }
}

#[embassy_executor::task]
async fn do_sd_card(
    mut spi_device: ExclusiveDevice<Spim<'static>, Output<'static>, Delay>,
    rng: Mutex<CriticalSectionRawMutex, Rng<'static, Async>>
) {
    loop {
        IMU_READY.wait().await;
        let sdcard = SdCard::new(&mut spi_device, Delay);

        let s = sdcard.num_bytes().unwrap();
        info!("SD card is {} bytes", s);

        let volume_mgr = VolumeManager::new(sdcard, DummyClock);
        let volume0 = volume_mgr.open_volume(VolumeIdx(0)).unwrap();
        let root_dir = volume0.open_root_dir().unwrap();

        let random_fname = rng.lock().await.blocking_next_u32();
        let mut my_file = root_dir
            .open_file_in_dir(format!(20; "LOG-{}.CSV", random_fname as u8).unwrap().as_str(), embedded_sdmmc::Mode::ReadWriteCreateOrTruncate)
            .unwrap();


        my_file.write(get_header_string().as_bytes()).unwrap(); // TODO: Replace with binary header
        let mut total = 0;

        let mut data = [0_u8;512];
        loop {
            if COMPLETE.signaled() && SAMPLES.is_empty() {
                COMPLETE.wait().await;
                break
            }
            let read = SAMPLES.read(&mut data).await;
            total += read;
            my_file.write(&data[..read]).unwrap();
        }

        info!("{}", total);
        my_file.flush().unwrap();
        my_file.close().unwrap();
        root_dir.close().unwrap();
        info!("Completed writing");
    }
}
