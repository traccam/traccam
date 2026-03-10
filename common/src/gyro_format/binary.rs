use core::num::NonZeroU64;
use zerocopy::{FromBytes, IntoBytes};

#[repr(C)]
#[derive(IntoBytes, FromBytes, Debug, PartialEq)]
pub struct BinGyroHeader {
	gps_start_ts: Option<NonZeroU64>,
	timescale: u64, // Timestamp resolution in seconds
}

impl BinGyroHeader {
	pub const fn new() -> Self {
		Self {
			gps_start_ts: None,
			timescale: 1_000_000,
		}
	}
}