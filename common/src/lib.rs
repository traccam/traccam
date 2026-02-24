#![cfg_attr(not(feature = "std"), no_std)]

use chrono::{FixedOffset, NaiveDate, NaiveTime};

pub mod display;
pub mod time;

#[derive(Clone, Default)]
pub struct DisplayState {
    time: NaiveTime,
    date: NaiveDate,
    display_tz: DisplayTZ,
    pub lat: f64,
    pub lon: f64,
    pub sats: u8,
    pub hdop: f32,
}

#[derive(Copy, Clone, Default, PartialEq)]
pub enum DisplayTZ {
    Local {
        fixed_offset: FixedOffset
    },
    #[default]
    Utc,
}