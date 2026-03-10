use std::fs;
use traccam_common::gyro_format;

fn main() {
    let p = "/run/media/flareflo/8430-A509/LOG.CSV";
    let data = fs::read(p).unwrap();
    let headerlen = gyro_format::text::HEADER_LEN;
    print!("{}", gyro_format::text::get_header_string());
    for (i, array_window) in data[headerlen..].chunks(12).enumerate() {
        if array_window.len() < 12 {
            break
        }
        let gx = i16::from_le_bytes([array_window[0], array_window[1]]) as f64 * 0.061 / 1000.0;
        let gy = i16::from_le_bytes([array_window[2], array_window[3]]) as f64 * 0.061 / 1000.0;
        let gz = i16::from_le_bytes([array_window[4], array_window[5]]) as f64 * 0.061 / 1000.0;
        let ax = i16::from_le_bytes([array_window[6], array_window[7]]) as f64 * 8.75 / 1000.0;
        let ay = i16::from_le_bytes([array_window[8], array_window[9]]) as f64 * 8.75 / 1000.0;
        let az = i16::from_le_bytes([array_window[10], array_window[11]]) as f64 * 8.75 / 1000.0;
        let ts = (i as f64 * 602.4096386) as u64;
        println!("{ts},{gx}, {gy}, {gz}, {ax}, {ay}, {az}");
    }
}
