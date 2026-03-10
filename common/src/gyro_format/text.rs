use core::fmt;
use heapless::String;

const REVISION: &str = "traccam_v1";
const ORIENTATION: &str = "XYZ"; // TODO: determine

const HEADER: [[&str; 2]; 8] = [
	["GYROFLOW IMU LOG", ""],
	["version", "1.3"],
	["id", "REVISION"],
	["orientation", ORIENTATION],
	["tscale", "0.000001"],
	["gscale", "0.0174532925"],
	["ascale", "1.0"],
	["t,gx,gy,gz,ax,ay,az", ""],

];
pub const HEADER_LEN: usize = {
	let mut len = 0;
	let mut i = 0;
	while i < HEADER.len() {
		len += HEADER[i][0].len() + HEADER[i][1].len() + 2; // + newline and comma
		i += 1;
	}
	len
};

pub fn get_header_string() -> String::<HEADER_LEN> {
	let mut header = String::new();
	write_header(&mut header).unwrap();
	header
}

fn write_header(w: &mut impl fmt::Write) -> Result<(), fmt::Error> {
	for [key, value] in HEADER {
		writeln!(w, "{key},{value}")?;
	}
	Ok(())
}

