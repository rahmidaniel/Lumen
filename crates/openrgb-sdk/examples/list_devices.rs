//! Connects to a local OpenRGB server and prints every controller.
//!
//! Start OpenRGB with `--server` first, then: `cargo run -p openrgb-sdk --example list_devices`

use openrgb_sdk::{Client, ModeFlags};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let (client, _events) = Client::connect_local("lumen list_devices").await?;
    println!("OpenRGB SDK protocol v{}", client.protocol_version());

    for dev in client.controllers().await? {
        println!(
            "\n[{:#010x}] {} — {} · {} LEDs · {} zones · {} modes (active {})",
            dev.id,
            dev.display_name,
            dev.kind.label(),
            dev.leds.len(),
            dev.zones.len(),
            dev.modes.len(),
            dev.active_mode
        );
        for z in &dev.zones {
            let matrix = z.matrix.as_ref().map(|m| format!("{}×{} matrix", m.width, m.height)).unwrap_or_default();
            println!("  zone {:<28} {:<7} {:>4} LEDs  {}", z.name, z.kind.label(), z.leds_count, matrix);
        }
        if std::env::args().any(|a| a == "--leds") {
            for (i, led) in dev.leds.iter().enumerate() {
                println!("  led {i:>3} {:?}", led.name);
            }
        }
        for (i, m) in dev.modes.iter().enumerate() {
            let save = if m.flags.contains(ModeFlags::MANUAL_SAVE) { " [saveable]" } else { "" };
            let per_led = if m.flags.contains(ModeFlags::HAS_PER_LED_COLOR) { " [per-led]" } else { "" };
            println!("  mode {i:>2} {:<24} flags={:#06x}{per_led}{save}", m.name, m.flags.0);
        }
    }
    Ok(())
}
