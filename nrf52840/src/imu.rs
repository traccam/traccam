use heapless::Vec;
use defmt::{error, info};
use embassy_nrf::{twim, Peri, Peripherals};
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pull};
use embassy_nrf::gpiote::{InputChannel, InputChannelPolarity};
use embassy_nrf::peripherals::{GPIOTE_CH0, P0_07, P0_11, P0_27, P1_08, TWISPI0};
use embassy_nrf::twim::Twim;
use embassy_time::{Duration, Instant, Timer};
use crate::Irqs;

/// Implementing the LSM6DS3TR-C IMU
/// https://www.st.com/resource/en/datasheet/lsm6ds3tr-c.pdf

const IMU_ADDR: u8 = 0x6A; // Default I2C address for LSM6DS3TR-C
const REG_FIFO_DATA_OUT_L: u8 = 0x3E; // Start of FIFO data register

pub struct ImuRessources {
	interrupt: InputChannel<'static>,
	imu_i2c: Twim<'static>,
	power: Output<'static>,
}

pub struct Imu {
	res: ImuRessources,
	fifo_buf: [u8; 4096],
	i2c_buf: [u8; 4096],
}

const FIFO_CTRL3: u8 = 0x08;
const FIFO_CTRL5: u8 = 0x0A;
const CTRL3_C: u8 = 0x12;
const FIFO_CTRL1: u8 = 0x06;
const FIFO_CTRL2: u8 = 0x07;
const INT1_CTRL: u8 = 0x0D;
const CTRL1_XL: u8 = 0x10;
const CTRL2_G: u8 = 0x11;

const WATERMARK_LIMIT: u8 = 250;

impl Imu {
	pub async fn init(
		mut res: ImuRessources,
	) -> Self {
		res.power.set_high();

		// Let it boot, manual says 3ms
		Timer::after_millis(5).await;

		let cmds = [
			[CTRL3_C, 0b01000100], //    BDU=1, IF_INC=1
			[FIFO_CTRL1, WATERMARK_LIMIT], // Watermark LSB
			[FIFO_CTRL2, 0x00], //       Watermark MSB = 0
			[FIFO_CTRL3, 0b00001001], // No decimation
			[FIFO_CTRL5, 0b01000110], // 1.66kHz, Continuous mode
			[INT1_CTRL, 0b00011000], //  Route FIFO threshold  and overrun to INT1
			[CTRL1_XL, 0b10000000], //   Accel 1.66kHz, 2g
			[CTRL2_G, 0b10000000], //    Gyro 1.66kHz, 250dps
		];

		for cmd in cmds {
			let ram_cmd = [cmd[0], cmd[1]]; // Forces the data onto the stack
			res.imu_i2c.write(IMU_ADDR, &ram_cmd).await.unwrap();
		}

		Self {
			res,
			fifo_buf: [0_u8; 4096],
			i2c_buf: [0_u8; 4096],
		}
	}

	pub async fn read_raw_samples(&mut self) -> &[u8; 4096] {
		let ram_reg = [REG_FIFO_DATA_OUT_L];
		self.res.imu_i2c.write_read(IMU_ADDR, &ram_reg, &mut self.fifo_buf).await.unwrap();
		&self.fifo_buf
	}

	pub async fn read_samples(&mut self, out: &mut Vec<Sample, 1000>) {
		out.clear();
		let (to_read, overrun) = self.fifo_status().await;
		let data = self.read_raw_samples().await;

		if overrun {
			panic!("FIFO overrun!");
		}
		defmt::assert!(to_read < out.capacity(), "Outvec overrun!");

		for chunk in data[..to_read].chunks_exact(12) {
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

			out.push(Sample {
				g_x,
				g_y,
				g_z,
				a_x,
				a_y,
				a_z,
			}).unwrap();
		}
	}

	pub async fn data_interrupt(&mut self)  {
		self.res.interrupt.wait().await
	}

	pub async fn fifo_status(&mut self) -> (usize, bool) {
		let mut status = [0u8; 2];
		let ram_reg = [0x3A]; // FIFO_STATUS1 (0x3A) and FIFO_STATUS2 (0x3B)

		self.res.imu_i2c.write_read(IMU_ADDR, &ram_reg, &mut status).await.unwrap();

		// Combine lower 8 bits and upper 4 bits
		let diff_fifo = (status[0] as u16) | (((status[1] & 0x0F) as u16) << 8);

		let bytes_to_read = (diff_fifo as usize) * 2;
		let overrun = (status[1] & 0x40) != 0;

		(bytes_to_read, overrun)
	}

	pub fn poweroff(mut self) -> ImuRessources {
		self.res.power.set_low();
		self.res
	}
}

impl ImuRessources {
	pub fn new(
		power_pin: Peri<'static, P1_08>,
		int1_pin: Peri<'static, P0_11>,
		gpiote: Peri<'static, GPIOTE_CH0>,
		twisp: Peri<'static, TWISPI0>,
		sda: Peri<'static, P0_07>,
		scl: Peri<'static, P0_27>,
	) -> Self {

		let power = Output::new(power_pin, Level::Low, OutputDrive::Standard);

		let mut interrupt = InputChannel::new(
			gpiote,
			int1_pin,
			Pull::None,
			InputChannelPolarity::LoToHi,
		);

		let mut twim_config = twim::Config::default();
		twim_config.frequency = twim::Frequency::K400;

		let mut imu_i2c = Twim::new(
			twisp,
			Irqs,
			sda,
			scl,
			twim_config,
			&mut [], // Only use ram-based commands until init completes!
		);

		Self {
			interrupt,
			imu_i2c,
			power,
		}
	}
}

#[derive(Debug)]
pub struct Sample {
	pub g_x: f32,
	pub g_y: f32,
	pub g_z: f32,

	pub a_x: f32,
	pub a_y: f32,
	pub a_z: f32,
}