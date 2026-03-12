use crate::Irqs;
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pull};
use embassy_nrf::gpiote::{InputChannel, InputChannelPolarity};
use embassy_nrf::peripherals::{GPIOTE_CH0, P0_07, P0_11, P0_27, P1_08, TWISPI0};
use embassy_nrf::twim::Twim;
use embassy_nrf::{Peri, twim};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::pipe::Pipe;
use embassy_time::Timer;
use heapless::Vec;
use static_cell::ConstStaticCell;

/// Implementing the LSM6DS3TR-C IMU
/// https://www.st.com/resource/en/datasheet/lsm6ds3tr-c.pdf

const IMU_ADDR: u8 = 0x6A; // Default I2C address for LSM6DS3TR-C
const REG_FIFO_DATA_OUT_L: u8 = 0x3E;
const REG_FIFO_DATA_OUT_H: u8 = 0x3F;

pub struct ImuRessources {
    interrupt: InputChannel<'static>,
    imu_i2c: Twim<'static>,
    power: Output<'static>,
}

pub struct Imu {
    res: ImuRessources,
    fifo_buf: [u8; 4096],
}

const FIFO_CTRL3: u8 = 0x08;
const FIFO_CTRL5: u8 = 0x0A;
const CTRL3_C: u8 = 0x12;
const FIFO_CTRL1: u8 = 0x06;
const FIFO_CTRL2: u8 = 0x07;
const INT1_CTRL: u8 = 0x0D;
const CTRL1_XL: u8 = 0x10;
const CTRL2_G: u8 = 0x11;

// Size of the FIFO buffer on the IMU
const FIFO_BUFSIZE: usize = 4096;

// A "Sample" is Accel[3] + Gyro[3] = 6 * i16 = 12 bytes
const BYTES_PER_SAMPLE: usize = 12;

// IMU onboard memory holds 4096 byte / (6 * 16 bit) = 341 samples
// At 1.66kHz we have to empty the FIFO 6.64kHz aka every 150.6 ms
// This means we can miss our readout target by at most 54.8ms
const WATERMARK_LIMIT: u8 = 250; // Amount of words
// Absolute maximum amount of samples that fit into 4096 byte buffer on the IMU
const FIFO_MAX_SAMPLES: usize = FIFO_BUFSIZE / BYTES_PER_SAMPLE;

const SAMPLE_FREQ: f32 = 1660.0;
pub const SAMPLE_INTERVAL_MICROS: f32 = 1000000.0 / SAMPLE_FREQ;

impl Imu {
    pub async fn init(mut res: ImuRessources) -> Self {
        // Ensure full reset and power discharge
        res.power.set_low();
        Timer::after_millis(200).await;
        res.power.set_high();

        // Let it boot, manual says 3ms, but our power Rail takes up to 300ms to rise (minimum)
        Timer::after_millis(500).await;

        let cmds = [
            [CTRL3_C, 0b01000100],         //         BDU=1, IF_INC=1
            [FIFO_CTRL1, WATERMARK_LIMIT], // Watermark LSB
            [FIFO_CTRL2, 0x00],            //            Watermark MSB = 0
            [FIFO_CTRL3, 0b00001001],      //      No decimation
            [FIFO_CTRL5, 0b0_1000_110],      //      1.66kHz, Continuous mode
            [INT1_CTRL, 0b00011000],       //       Route FIFO threshold  and overrun to INT1
            [CTRL1_XL, 0b1000_00_0_0],        //        Accel 1.66kHz, 2g
            [CTRL2_G, 0b1000_00_0_0],         //         Gyro 1.66kHz, 250dps
        ];

        for cmd in cmds {
            res.imu_i2c.write(IMU_ADDR, &cmd).await.unwrap();
        }

        Self {
            res,
            fifo_buf: [0_u8; FIFO_BUFSIZE],
        }
    }

    pub async fn read_raw_samples(&mut self, to_read: usize) -> &[u8; FIFO_BUFSIZE] {
        let ram_reg = [REG_FIFO_DATA_OUT_L]; // Start of FIFO data register
        self.res
            .imu_i2c
            // Important to read only exactly as much as needed, otherwise the FIFO goes haywire
            .write_read(IMU_ADDR, &ram_reg, &mut self.fifo_buf[..to_read])
            .await
            .unwrap();
        &self.fifo_buf
    }

    pub async fn read_samples<M: RawMutex, const N: usize>(&mut self, out: &Pipe<M, N>) {
        let fifo_status = self.fifo_status().await;
        defmt::assert!(!fifo_status.overrun, "FIFO overrun!");

        let data = self.read_raw_samples(fifo_status.unread_bytes()).await;

        out.write_all(&data[..fifo_status.unread_bytes()]).await;
    }

    pub async fn data_interrupt(&mut self) {
        self.res.interrupt.wait().await
    }

    pub async fn fifo_status(&mut self) -> FifoStatus {
        let mut status = [0u8; 2];
        let ram_reg = [0x3A]; // FIFO_STATUS1 (0x3A) and FIFO_STATUS2 (0x3B)

        self.res
            .imu_i2c
            .write_read(IMU_ADDR, &ram_reg, &mut status)
            .await
            .unwrap();

        FifoStatus {
            below_watermark: (status[1] & 0b_1000_0000) == 0,
            overrun: (status[1] & 0b_0100_0000) == 1,
            fifo_full_smart: (status[1] & 0b_0010_0000) == 1,
            fifo_empty: (status[1] & 0b_0001_0000) == 1,
            unread_bytes: ((status[0] as u16) | (((status[1] & 0x0F) as u16) << 8)) * 2, // It returns amount of words, each word is 16 bit
        }
    }

    pub fn poweroff(mut self) -> ImuRessources {
        self.res.power.set_low();
        self.res
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FifoStatus {
    below_watermark: bool,
    overrun: bool,
    fifo_full_smart: bool,
    fifo_empty: bool,
    unread_bytes: u16,
}

impl FifoStatus {
    pub fn unread_bytes(&self) -> usize {
        self.unread_bytes as usize
    }
}

static TX_BUFFER: ConstStaticCell<[u8; 512]> = ConstStaticCell::new([0_u8; 512]);

impl ImuRessources {
    pub fn new(
        power_pin: Peri<'static, P1_08>,
        int1_pin: Peri<'static, P0_11>,
        gpiote: Peri<'static, GPIOTE_CH0>,
        twisp: Peri<'static, TWISPI0>,
        sda: Peri<'static, P0_07>,
        scl: Peri<'static, P0_27>,
    ) -> Self {
        // IMU draws quite some power at start, we need to use HighDrive
        let power = Output::new(power_pin, Level::Low, OutputDrive::HighDrive);

        let interrupt =
            InputChannel::new(gpiote, int1_pin, Pull::None, InputChannelPolarity::LoToHi);

        let mut twim_config = twim::Config::default();
        twim_config.frequency = twim::Frequency::K400;

        let imu_i2c = Twim::new(twisp, Irqs, sda, scl, twim_config, TX_BUFFER.take());

        Self {
            interrupt,
            imu_i2c,
            power,
        }
    }
}