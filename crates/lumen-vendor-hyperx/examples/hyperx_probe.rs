//! Diagnostic: on the HyperX vendor interface, send the battery request several
//! times and dump every input report and feature report that can be read back.
//!
//! `cargo run -p lumen-vendor-hyperx --example hyperx_probe -- 0x0f98`

use std::time::{Duration, Instant};

use hidapi::HidApi;
use lumen_vendor_hyperx::VENDOR_ID;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pid_arg = std::env::args().nth(1).unwrap_or_else(|| "0x0f98".into());
    let pid = u16::from_str_radix(pid_arg.trim_start_matches("0x"), 16)?;
    let api = HidApi::new()?;

    for info in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid) {
        if info.usage_page() < 0xFF00 {
            continue; // only vendor-defined collections accept the request
        }
        println!("\niface mi={} usage_page={:#06x} usage={:#06x}\n  {}", info.interface_number(), info.usage_page(), info.usage(), info.path().to_string_lossy());
        let dev = api.open_path(info.path())?;
        dev.set_blocking_mode(false)?;

        // Drain anything already queued.
        let mut junk = [0u8; 128];
        while matches!(dev.read(&mut junk), Ok(n) if n > 0) {}

        // What feature reports exist? Try IDs 0x50..=0x53 before we send anything.
        for id in [0x50u8, 0x51, 0x52, 0x53] {
            let mut buf = [0u8; 64];
            buf[0] = id;
            match dev.get_feature_report(&mut buf) {
                Ok(n) => println!("  feature {id:#04x} before: {n} bytes {:02x?}", &buf[..n.min(16)]),
                Err(e) => println!("  feature {id:#04x} before: {e}"),
            }
        }

        let mut req = [0u8; 64];
        req[0] = 0x50;
        req[1] = 0x02;
        for round in 1..=3 {
            match dev.write(&req) {
                Ok(n) => println!("  round {round}: write accepted ({n} bytes)"),
                Err(e) => {
                    println!("  round {round}: write failed: {e}");
                    break;
                }
            }
            let deadline = Instant::now() + Duration::from_millis(1500);
            let mut got = 0;
            while Instant::now() < deadline {
                let mut buf = [0u8; 128];
                match dev.read_timeout(&mut buf, 100) {
                    Ok(0) => {}
                    Ok(n) => {
                        got += 1;
                        println!("    input {n} bytes: {:02x?}", &buf[..n.min(24)]);
                    }
                    Err(e) => {
                        println!("    read error: {e}");
                        break;
                    }
                }
            }
            if got == 0 {
                println!("    (no input report within 1.5 s)");
            }
            for id in [0x50u8, 0x51, 0x52] {
                let mut buf = [0u8; 64];
                buf[0] = id;
                if let Ok(n) = dev.get_feature_report(&mut buf) {
                    println!("    feature {id:#04x} after: {n} bytes {:02x?}", &buf[..n.min(16)]);
                }
            }
        }
    }
    Ok(())
}
