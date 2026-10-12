//! Change the polling rate, sleep timer and low-battery threshold, read them
//! back, then restore them. Pause anything streaming to the device first.
//!
//!   cargo run -p lumen-vendor-razer --example razer_settings_roundtrip -- 0296

use hidapi::HidApi;
use lumen_vendor_razer::RazerDevice;

fn main() {
    let pid = std::env::args().nth(1).and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()).expect("usage: <pid hex>");
    let api = HidApi::new().expect("hidapi");
    let dev = RazerDevice::open(&api, pid).expect("open");

    let (kind, hz) = dev.polling_rate().expect("polling");
    let idle = dev.idle_time().expect("idle");
    let threshold = dev.low_battery_threshold().expect("threshold");
    println!("before:   {hz} Hz ({kind:?}), idle {idle} s, threshold {threshold:#04x}");

    let other_hz = if hz == 500 { 1000 } else { 500 };
    let other_idle = if idle == 600 { 900 } else { 600 };
    let other_threshold = if threshold == 0x26 { 0x0C } else { 0x26 };
    let r1 = dev.set_polling_rate(kind, other_hz);
    let r2 = dev.set_idle_time(other_idle);
    let r3 = dev.set_low_battery_threshold(other_threshold);
    println!("writes:   polling {r1:?}, idle {r2:?}, threshold {r3:?}");
    println!("changed:  {:?} Hz, idle {:?} s, threshold {:?}", dev.polling_rate().map(|p| p.1), dev.idle_time(), dev.low_battery_threshold());

    let r1 = dev.set_polling_rate(kind, hz);
    let r2 = dev.set_idle_time(idle);
    let r3 = dev.set_low_battery_threshold(threshold);
    println!("restores: polling {r1:?}, idle {r2:?}, threshold {r3:?}");
    println!("after:    {:?} Hz, idle {:?} s, threshold {:?}", dev.polling_rate().map(|p| p.1), dev.idle_time(), dev.low_battery_threshold());
}
