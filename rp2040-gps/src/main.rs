#![no_std]
#![no_main]

use embedded_hal_bus::spi::ExclusiveDevice;
use traccam_common::display::draw_status_display;
use traccam_common::DisplayState;
use embedded_graphics::prelude::{DrawTarget};
use embedded_graphics::pixelcolor::BinaryColor;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use defmt::*;
use defmt::export::display;
use ssd1306::prelude::*;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, i2c, spi, Peri, Peripherals};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::{I2C1, PIN_2, PIN_3, PIN_4, PIN_5, PIN_6, PIN_7, SPI0, UART0};
use embassy_rp::spi::Spi;
use embassy_rp::uart::{BufferedUartRx, BufferedInterruptHandler};
use embassy_rp::uart;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Timer;
use heapless::{String};
use nmea::Nmea;
use embassy_sync::signal::Signal;
use embedded_sdmmc::{Mode, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager};
use embassy_time::Delay;

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hi");
    let p = embassy_rp::init(Default::default());

    spawner.spawn(do_sd_card(p.SPI0, p.PIN_4, p.PIN_5, p.PIN_2, p.PIN_3)).unwrap();

    let mut i2cc = i2c::Config::default();
    i2cc.frequency = 400_000;
    let t = do_display(I2c::new_async(p.I2C1, p.PIN_27, p.PIN_26, Irqs, i2cc));
    spawner.spawn(t).unwrap();

    let mut config = uart::Config::default();
    config.baudrate = 115200;

    let mut rx_buf = [0u8; 4096];

    let mut uart_rx = BufferedUartRx::new(p.UART0, Irqs, p.PIN_1, &mut rx_buf, config);

    let mut nmea = Nmea::default();
    let mut buffer: String<128> = String::new();

    loop {
        let mut byte = [0u8; 1];

        match embedded_io_async::Read::read(&mut uart_rx, &mut byte).await {
            Ok(_b) => {
                let b = byte[0] as char;

                if b == '\n' {
                    if let Ok(mtype) = nmea.parse(buffer.as_str()) {
                        let mut state = DisplayState::default();
                        let sats = nmea.satellites();
                        state.sats = sats.len() as _;

                        // Checking only for latitude and longitude
                        if let (Some(lat), Some(lon)) = (nmea.latitude, nmea.longitude) {
                            state.lat = lat;
                            state.lon = lon;
                        }

                        if let Some(date) = nmea.fix_date {
                            state.update_date(date);
                        }

                        if let Some(time) = nmea.fix_time {
                            state.update_utc_time(time);
                        }

                        if let Some(hdop) = nmea.hdop() {
                            state.hdop = hdop;
                        }

                        DISPLAY_SIGNAL.signal(state);
                    }
                    buffer.clear();
                } else if b != '\r' {
                    let _ = buffer.push(b);
                }
            }
            Err(e) => {
                error!("UART Read Error: {:?}", e);
            }
        }
    }
}

struct DummyClock;

impl TimeSource for DummyClock {
    fn get_timestamp(&self) -> Timestamp {
        // Returns a dummy date: Jan 1, 2024, 00:00:00
        Timestamp::from_calendar(2024, 1, 1, 0, 0, 0).unwrap()
    }
}

#[embassy_executor::task]
async fn do_sd_card(spi: Peri<'static, SPI0>, miso: Peri<'static, PIN_4>, cs_pin: Peri<'static, PIN_5>, clk: Peri<'static, PIN_2>, mosi: Peri<'static, PIN_3>) {
    let mut cs = Output::new(cs_pin, Level::High);

    let mut spi_config = spi::Config::default();
    spi_config.frequency = 10_000_000;

    let spi_bus = Spi::new_blocking(
        spi,
        clk,
        mosi,
        miso,
        spi_config,
    );

    let spi_device = ExclusiveDevice::new(spi_bus, cs, Delay).unwrap();


    let mut sdcard = SdCard::new(spi_device, Delay);

    let s = sdcard.num_bytes().unwrap();
    info!("SD card is {} bytes", s);

    let mut volume_mgr = VolumeManager::new(sdcard, DummyClock);
    let mut volume0 = volume_mgr.open_volume(VolumeIdx(0)).unwrap();
    let mut root_dir = volume0.open_root_dir().unwrap();

    let mut my_file = root_dir.open_file_in_dir("TEST.TXT", Mode::ReadOnly).unwrap();

    while !my_file.is_eof() {
        let mut buffer = [0u8; 32];
        let num_read = my_file.read(&mut buffer).unwrap();

        // Convert raw bytes to a UTF-8 string so defmt can print it to the terminal
        let text_chunk = core::str::from_utf8(&buffer[..num_read]).unwrap();

        // Print the chunk (defmt might add newlines per chunk, but it gets the data out)
        info!("{}", text_chunk);
    }

    my_file.close().unwrap();
    root_dir.close().unwrap();
}

static DISPLAY_SIGNAL: Signal<CriticalSectionRawMutex, DisplayState> = Signal::new();
#[embassy_executor::task]
async fn do_display(i2c: I2c<'static, I2C1, Async>) {
    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(
        interface,
        DisplaySize128x32,
        DisplayRotation::Rotate0,
    ).into_buffered_graphics_mode();
    display.init().unwrap();

    let mut state = DisplayState::default();
    loop {
        display.clear(BinaryColor::Off).unwrap();

        if DISPLAY_SIGNAL.signaled() {
            let d = DISPLAY_SIGNAL.wait().await;
            state = d;
        };

        draw_status_display(&mut display, &state);

        display.flush().unwrap();
        Timer::after_millis(40).await;
    }
}