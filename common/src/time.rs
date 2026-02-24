use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use crate::DisplayState;

impl DisplayState {
	//
	pub fn update_date(&mut self, d: NaiveDate) {
		self.date = d;
	}

	/// This must be GPS UTC time!
	pub fn update_utc_time(&mut self, t: NaiveTime) {
		self.time = t;
	}

	pub fn now(&self) -> DateTime<Utc> {
		DateTime::from_naive_utc_and_offset(NaiveDateTime::new(self.date, self.time), Utc)
	}
}