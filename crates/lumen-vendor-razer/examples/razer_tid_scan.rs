//! Diagnostic: for one Razer product, find the interface that accepts the
//! control report and try a range of transaction IDs with generous waits,
//! printing every raw reply. Read-only (firmware-version query).
//!
//! `cargo run -p lumen-vendor-razer --example razer_tid_scan -- 0x0296`

use std::thread::sleep;
use std::time::Duration;

use hidapi::HidApi;
use lumen_vendor_razer::protocol::{get_firmware_version, Report, REPORT_LEN};
use lumen_vendor_razer::VENDOR_ID;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pid_arg = std::env::args().nth(1).unwrap_or_else(|| "0x0296".into());
    let pid = u16::from_str_radix(pid_arg.trim_start_matches("0x"), 16)?;
    let api = HidApi::new()?;

    for info in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == pid) {
        let path = info.path().to_string_lossy().into_owned();
        let Ok(dev) = api.open_path(info.path()) else { continue };

        // Does this interface take the 90-byte report at all?
        let mut probe = [0u8; REPORT_LEN + 1];
        probe[1..].copy_from_slice(&get_firmware_version().encode());
        if dev.send_feature_report(&probe).is_err() {
            continue;
        }
        println!("\ncontrol-capable interface: mi={} usage_page={:#06x} usage={:#06x}\n  {path}", info.interface_number(), info.usage_page(), info.usage());

        for tid in [0x1Fu8, 0x3F, 0x9F, 0xFF, 0x08, 0x00, 0x88, 0x1A, 0x80] {
            let mut report = get_firmware_version();
            report.transaction_id = tid;
            let mut out = [0u8; REPORT_LEN + 1];
            out[1..].copy_from_slice(&report.encode());
            dev.send_feature_report(&out)?;

            let mut seen = Vec::new();
            for step in 0..12 {
                sleep(Duration::from_millis(if step == 0 { 2 } else { 8 }));
                let mut buf = [0u8; REPORT_LEN + 1];
                dev.get_feature_report(&mut buf)?;
                let Some(r) = Report::decode(&buf[1..]) else { continue };
                let line = format!(
                    "status={:?} tid={:#04x} class={:#04x} cmd={:#04x} size={} args={:02x?}",
                    r.status(),
                    r.transaction_id,
                    r.class,
                    r.command,
                    r.data_size,
                    &r.args[..6]
                );
                if seen.last() != Some(&line) {
                    seen.push(line);
                }
                if r.transaction_id == tid && r.class == 0 && r.command == 0x81 && r.status() == lumen_vendor_razer::Status::Ok {
                    break;
                }
            }
            println!("  tid {tid:#04x}:");
            for l in seen {
                println!("      {l}");
            }
        }
    }
    Ok(())
}
