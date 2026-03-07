#![no_std]
#![no_main]

mod imu;

use defmt::info;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::gpiote::{InputChannel, InputChannelPolarity};
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::bind_interrupts;
use embassy_time::{Duration, Instant, Timer};
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

        let samples = imu.read_samples().await;

        // 6 bytes for Gyro, 6 bytes for Accel = 12 bytes per dataset
        for chunk in samples.chunks_exact(12).take(1) {
            if last_print + Duration::from_millis(100) > Instant::now() {
                continue
            }
            defmt::info!("\x1B[2J\x1B[H");
            last_print = Instant::now();
            let g_x_raw = i16::from_le_bytes([chunk[0], chunk[1]]);
            let g_y_raw = i16::from_le_bytes([chunk[2], chunk[3]]);
            let g_z_raw = i16::from_le_bytes([chunk[4], chunk[5]]);

            let a_x_raw = i16::from_le_bytes([chunk[6], chunk[7]]);
            let a_y_raw = i16::from_le_bytes([chunk[8], chunk[9]]);
            let a_z_raw = i16::from_le_bytes([chunk[10], chunk[11]]);

            let a_x = (a_x_raw as f32) * 0.061 / 1000.0;
            let a_y = (a_y_raw as f32) * 0.061 / 1000.0;
            let a_z = (a_z_raw as f32) * 0.061 / 1000.0;

            let g_x = (g_x_raw as f32) * 8.75 / 1000.0;
            let g_y = (g_y_raw as f32) * 8.75 / 1000.0;
            let g_z = (g_z_raw as f32) * 8.75 / 1000.0;

            info!("Accel (g): {}, {}, {} | Gyro (dps): {}, {}, {}", a_x, a_y, a_z, g_x, g_y, g_z);
        }

        led.set_low();
    }
}