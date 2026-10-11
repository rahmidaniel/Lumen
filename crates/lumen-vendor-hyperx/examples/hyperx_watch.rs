//! Diagnostic: poll the battery request every few seconds and log every
//! `51 02` reply with a timestamp, so field meanings can be inferred from how
//! the bytes change while the mouse is used, plugged in and unplugged.
//!
//! `cargo run -p lumen-vendor-hyperx --example hyperx_watch -- 0x0f98 300`

use std::time::{Duration, Instant};

use hidapi::HidApi;
use lumen_vendor_hyperx::VENDOR_ID;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pid_arg = std::env::args().nth(1).unwrap_or_else(|| "0x0f98".into());
    let total: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let pid = u16::from_str_radix(pid_arg.trim_start_matches("0x"), 16)?;
    let api = HidApi::new()?;
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid && d.usage_page() >= 0xFF00)
        .ok_or("vendor interface not found")?;
    let dev = api.open_path(info.path())?;

    let mut req = [0u8; 64];
    req[0] = 0x50;
    req[1] = 0x02;
    let start = Instant::now();
    let mut last: Option<Vec<u8>> = None;
    println!("t(s)   reply (first 12 bytes after 51 02)        +2    +4    +6..7 LE");
    while start.elapsed() < Duration::from_secs(total) {
        let _ = dev.write(&req);
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            let mut buf = [0u8; 128];
            let n = match dev.read_timeout(&mut buf, 100) {
                Ok(n) => n,
                Err(_) => break,
            };
            if n < 12 || buf[0] != 0x51 || buf[1] != 0x02 {
                continue;
            }
            let bytes = buf[..12].to_vec();
            if last.as_ref() != Some(&bytes) {
                let word = u16::from_le_bytes([buf[6], buf[7]]);
                println!(
                    "{:6.1} {:02x?}  {:3}  {:3}  {:5} (0x{:04x})",
                    start.elapsed().as_secs_f64(),
                    &buf[2..12],
                    buf[2],
                    buf[4],
                    word,
                    word
                );
                last = Some(bytes);
            }
        }
        std::thread::sleep(Duration::from_secs(4));
    }
    println!("done");
    Ok(())
}
