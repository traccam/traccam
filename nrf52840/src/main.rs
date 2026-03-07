#![no_std]
#![no_main]

mod imu;

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::gpiote::{InputChannel, InputChannelPolarity};
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::bind_interrupts;
use embassy_time::{Duration, Instant, Timer};
use heapless::Vec;
use {defmt_rtt as _, panic_probe as _};
use crate::imu::{Imu, ImuRessources};

bind_interrupts!(struct Irqs {
    TWISPI0 => twim::InterruptHandler<embassy_nrf::peripherals::TWISPI0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    let mut led = Output::new(p.P0_13, Level::Low, OutputDrive::Standard);


    let resources = ImuRessources::new(
        p.P1_08,
        p.P0_11,
        p.GPIOTE_CH0,
        p.TWISPI0,
        p.P0_07,
        p.P0_27,
    );
    let mut imu = Imu::init(resources).await;

    let mut last_print = Instant::now();

    loop {
        imu.data_interrupt().await;

        led.set_high();

        let mut samples = Vec::new();
        imu.read_samples(&mut samples).await;

        for sample in samples {
            if last_print + Duration::from_millis(24) > Instant::now() {
                continue
            }
            defmt::info!("\x1B[2J\x1B[H");
            last_print = Instant::now();

            info!("Accel (g):\n {}\n{}\n{}\nGyro (dps):\n {}\n{}\n{}\n", sample.a_x, sample.a_y, sample.a_z, sample.g_x, sample.g_y, sample.g_z);
        }

        led.set_low();
    }

    // Softhalt
    loop {
        Timer::after_secs(100).await;
    }
}