#![cfg_attr(not(feature = "std"), no_std)]

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};

pub mod display;
pub mod time;
pub mod sd_storage;
pub mod gyro_format;

#[derive(Clone, Default)]
pub struct DisplayState {
    time: NaiveTime,
    date: NaiveDate,
    pub local_time: Option<DateTime<FixedOffset>>,
    pub lat: f64,
    pub lon: f64,
    pub sats: u8,
    pub hdop: f32,
}