use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};
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

	pub fn now_utc(&self) -> DateTime<FixedOffset> {
		DateTime::<Utc>::from_naive_utc_and_offset(NaiveDateTime::new(self.date, self.time), Utc).with_timezone(&FixedOffset::east_opt(0).expect("Infallible. UTC."))
	}

	pub fn now_local(&self) -> Option<DateTime<FixedOffset>> {
		self.local_time
	}
}