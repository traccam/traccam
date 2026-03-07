use defmt::info;
use embassy_nrf::{twim, Peri, Peripherals};
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pull};
use embassy_nrf::gpiote::{InputChannel, InputChannelPolarity};
use embassy_nrf::peripherals::{GPIOTE_CH0, P0_07, P0_11, P0_27, P1_08, TWISPI0};
use embassy_nrf::twim::Twim;
use embassy_time::Timer;
use crate::Irqs;

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

impl Imu {
	pub async fn init(
		mut res: ImuRessources,
	) -> Self {
		res.power.set_high();

		// Let it boot, manual says 3ms
		Timer::after_millis(5).await;

		let cmds = [
			[0x12, 0x44], // CTRL3_C: BDU=1, IF_INC=1
			[0x06, 0xFA], // FIFO_CTRL1: Watermark LSB = 250
			[0x07, 0x00], // FIFO_CTRL2: Watermark MSB = 0
			[0x08, 0x09], // FIFO_CTRL3: No decimation
			[0x0A, 0x46], // FIFO_CTRL5: 1.66kHz, Continuous mode
			[0x0D, 0x08], // INT1_CTRL: Route FIFO threshold to INT1
			[0x10, 0x80], // CTRL1_XL: Accel 1.66kHz, 2g
			[0x11, 0x80], // CTRL2_G: Gyro 1.66kHz, 250dps
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

	pub async fn read_samples(&mut self) -> &[u8; 4096] {
		let ram_reg = [REG_FIFO_DATA_OUT_L];
		self.res.imu_i2c.write_read(IMU_ADDR, &ram_reg, &mut self.fifo_buf).await.unwrap();
		&self.fifo_buf
	}
	pub async fn data_interrupt(&mut self)  {
		self.res.interrupt.wait().await
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