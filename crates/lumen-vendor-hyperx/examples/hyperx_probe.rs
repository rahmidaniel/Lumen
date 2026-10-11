//! Diagnostic: on the HyperX vendor interface, send the battery request and
//! record every distinct input report for a few seconds (first sighting time,
//! count), so the receiver's traffic pattern can be read at a glance.
//!
//! `cargo run -p lumen-vendor-hyperx --example hyperx_probe -- 0x0f98 [seconds]`

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use hidapi::HidApi;
use lumen_vendor_hyperx::VENDOR_ID;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pid_arg = std::env::args().nth(1).unwrap_or_else(|| "0x0f98".into());
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let pid = u16::from_str_radix(pid_arg.trim_start_matches("0x"), 16)?;
    let api = HidApi::new()?;

    for info in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid && d.usage_page() >= 0xFF00) {
        println!("iface mi={} usage_page={:#06x}\n  {}", info.interface_number(), info.usage_page(), info.path().to_string_lossy());
        let dev = api.open_path(info.path())?;

        // Baseline: what arrives without asking?
        let mut seen: BTreeMap<Vec<u8>, (f64, usize)> = BTreeMap::new();
        let start = Instant::now();
        let record = |seen: &mut BTreeMap<Vec<u8>, (f64, usize)>, buf: &[u8]| {
            let key = buf[..buf.len().min(20)].to_vec();
            let e = seen.entry(key).or_insert((start.elapsed().as_secs_f64(), 0));
            e.1 += 1;
        };
        let baseline_until = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < baseline_until {
            let mut buf = [0u8; 128];
            if let Ok(n) = dev.read_timeout(&mut buf, 100) {
                if n > 0 {
                    record(&mut seen, &buf[..n]);
                }
            }
        }
        println!("  unsolicited traffic in 1.5 s:");
        for (k, (t, c)) in &seen {
            println!("    t={t:5.2}s ×{c:<4} {k:02x?}");
        }

        let mut req = [0u8; 64];
        req[0] = 0x50;
        req[1] = 0x02;
        dev.write(&req)?;
        let sent = start.elapsed().as_secs_f64();
        println!("  request 50 02 sent at t={sent:5.2}s; listening {secs} s:");
        let mut after: BTreeMap<Vec<u8>, (f64, usize)> = BTreeMap::new();
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            let mut buf = [0u8; 128];
            if let Ok(n) = dev.read_timeout(&mut buf, 100) {
                if n > 0 {
                    record(&mut after, &buf[..n]);
                }
            }
        }
        for (k, (t, c)) in &after {
            let new = if seen.contains_key(k) { "" } else { "  <- new after request" };
            println!("    t={t:5.2}s ×{c:<4} {k:02x?}{new}");
        }
    }
    Ok(())
}
