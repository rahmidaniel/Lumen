//! Change the polling rate and DPI stages, read them back, then restore the
//! original table: a write test that leaves the mouse as it was.
//!
//!   cargo run -p lumen-vendor-hyperx --example hyperx_roundtrip -- 0f98

use hidapi::HidApi;
use lumen_vendor_hyperx::config::{read_config, write_config};

fn main() {
    let pid = std::env::args().nth(1).and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0x0F98);
    let api = HidApi::new().expect("hidapi");
    let original = read_config(&api, pid).expect("read");
    println!("before:  {} Hz, stages {:?}, active {}", original.polling_hz(), original.stages(), original.active());

    let mut changed = original.clone();
    changed.set_polling_hz(500).unwrap();
    let three: Vec<u32> = original.stages().into_iter().take(3).collect();
    changed.set_stages(&three, 0).unwrap();
    match write_config(&api, pid, &changed) {
        Ok(c) => println!("changed: {} Hz, stages {:?}, active {}", c.polling_hz(), c.stages(), c.active()),
        Err(e) => println!("change failed: {e}"),
    }

    let mut restore = original.clone();
    restore.set_stages(&original.stages(), original.active()).unwrap();
    match write_config(&api, pid, &restore) {
        Ok(c) => println!("restored: {} Hz, stages {:?}, active {}", c.polling_hz(), c.stages(), c.active()),
        Err(e) => println!("RESTORE FAILED: {e} — table was {:?}", original),
    }
}
