//! Lists attached Razer products and, for each, prints firmware, serial,
//! device mode and (if it has one) battery state. Read-only.
//!
//! `cargo run -p lumen-vendor-razer --example razer_probe`

use hidapi::HidApi;
use lumen_vendor_razer::RazerDevice;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let api = HidApi::new()?;
    for d in api.device_list().filter(|d| d.vendor_id() == lumen_vendor_razer::VENDOR_ID) {
        println!(
            "iface pid={:#06x} mi={} usage_page={:#06x} usage={:#06x} path={}",
            d.product_id(),
            d.interface_number(),
            d.usage_page(),
            d.usage(),
            d.path().to_string_lossy()
        );
    }
    let products = RazerDevice::list(&api);
    if products.is_empty() {
        println!("no Razer devices attached");
        return Ok(());
    }
    for info in products {
        println!("\n{} ({:#06x}) serial={} interfaces={}", info.product, info.pid, info.serial.as_deref().unwrap_or("?"), info.interfaces);
        let dev = match RazerDevice::open(&api, info.pid) {
            Ok(d) => d,
            Err(e) => {
                println!("  cannot open control interface: {e}");
                continue;
            }
        };
        println!("  transaction id {:#04x}", dev.transaction_id());
        match dev.firmware_version() {
            Ok((major, minor)) => println!("  firmware v{major}.{minor}"),
            Err(e) => println!("  firmware: {e}"),
        }
        match dev.serial() {
            Ok(s) => println!("  serial {s}"),
            Err(e) => println!("  serial: {e}"),
        }
        match dev.device_mode() {
            Ok(m) => println!("  device mode {m:#04x}"),
            Err(e) => println!("  device mode: {e}"),
        }
        match dev.battery() {
            Ok(b) => println!("  battery {}%{}", b.percent, if b.charging { " (charging)" } else { "" }),
            Err(e) => println!("  battery: {e}"),
        }
    }
    Ok(())
}
