//! Read (never write) the device settings a Razer product answers to:
//! polling rate, DPI, idle time and low-battery threshold.
//!
//!   cargo run -p lumen-vendor-razer --example razer_settings_probe -- 0296
//!
//! Pause anything streaming to the device first (Lumen: the pause button):
//! its frames overwrite the device's single reply buffer.

use hidapi::HidApi;
use lumen_vendor_razer::{RazerDevice, Report};

fn main() {
    let pid = std::env::args().nth(1).and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()).expect("usage: razer_settings_probe <pid hex>");
    let api = HidApi::new().expect("hidapi");
    let dev = RazerDevice::open(&api, pid).expect("open");
    println!("pid {pid:#06x} tid {:#04x}", dev.transaction_id());
    let queries: [(&str, u8, u8, u8); 6] = [
        ("polling rate", 0x00, 0x85, 0x01),
        ("polling rate (HyperPolling)", 0x00, 0xC0, 0x02),
        ("dpi x/y", 0x04, 0x85, 0x07),
        ("dpi stages", 0x04, 0x86, 0x26),
        ("idle time", 0x07, 0x83, 0x02),
        ("low-battery threshold", 0x07, 0x81, 0x01),
    ];
    for (name, class, command, size) in queries {
        let mut report = Report::new(class, command, size);
        if command == 0x85 && class == 0x04 || command == 0x86 {
            report = report.with_args(&[0x01]); // VARSTORE
        }
        match dev.exchange(report, 5) {
            Ok(reply) => println!("{name:<28} ok   size {:>2}  args {:02x?}", reply.data_size, &reply.args[..usize::from(size).min(24)]),
            Err(e) => println!("{name:<28} {e}"),
        }
    }
}
