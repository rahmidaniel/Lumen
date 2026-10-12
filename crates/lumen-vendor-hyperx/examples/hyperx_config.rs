//! Read (never write) a HyperX mouse's polling rate and DPI table: send
//! `32 02` on the vendor interface and print the `33 02` reply.
//!
//!   cargo run -p lumen-vendor-hyperx --example hyperx_config -- 0f98

use std::time::{Duration, Instant};

use hidapi::HidApi;

fn main() {
    let pid = std::env::args().nth(1).and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0x0F98);
    let api = HidApi::new().expect("hidapi");
    for info in api.device_list().filter(|d| d.vendor_id() == 0x03F0 && d.product_id() == pid && d.usage_page() >= 0xFF00) {
        println!("interface {} usage {:04x}/{:04x}", info.interface_number(), info.usage_page(), info.usage());
        let Ok(dev) = api.open_path(info.path()) else { continue };
        let mut request = [0u8; 64];
        request[0] = 0x32;
        request[1] = 0x02;
        if let Err(e) = dev.write(&request) {
            println!("  write failed: {e}");
            continue;
        }
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            let mut buf = [0u8; 128];
            let n = dev.read_timeout(&mut buf, 100).unwrap_or(0);
            if n == 0 {
                continue;
            }
            let r = &buf[..n];
            if r[0] == 0xFF && n > 16 && r[14] == 0x32 && r[15] == 0x02 {
                println!("  ack result {:#04x}", r[16]);
            } else if r[0] == 0x33 && r[1] == 0x02 {
                println!("  reply {:02x?}", &r[..40.min(n)]);
                println!("  polling divisor {:#04x} = {} Hz, stage mask {:#04x}, active stage {}", r[2], if r[2] > 0 { 8000 / u32::from(r[2]) } else { 0 }, r[3], r[4]);
                for stage in 0..5 {
                    let o = 5 + stage * 5;
                    let code = u16::from_le_bytes([r[o], r[o + 1]]);
                    println!("  stage {stage}: {} dpi, colour #{:02x}{:02x}{:02x}, enabled {}", (u32::from(code) + 1) * 50, r[o + 2], r[o + 3], r[o + 4], (r[3] >> stage) & 1 == 1);
                }
                return;
            }
        }
        println!("  no 33 02 reply");
    }
}
