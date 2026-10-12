//! Low-battery notifications: one when a wireless device drops to 20 %, one
//! more at 5 %, nothing while it charges, and the cycle starts over once it
//! has been charged above 25 %.

use std::collections::HashMap;

use lumen_core::Battery;

const LOW: u8 = 20;
const CRITICAL: u8 = 5;
/// Back above this (or charging) and the device may warn again later.
const RESET: u8 = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Low,
    Critical,
}

/// A notification to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub title: String,
    pub body: String,
}

/// Remembers which warning each device has had. Devices are keyed by
/// something that survives OpenRGB restarts (their location), not by ID.
#[derive(Default)]
pub struct BatteryAlerts {
    warned: HashMap<String, Level>,
}

impl BatteryAlerts {
    /// Feed the latest reading of one device; returns a notification when one is due.
    pub fn check(&mut self, key: &str, name: &str, battery: Battery) -> Option<Alert> {
        if battery.charging || battery.percent > RESET {
            self.warned.remove(key);
            return None;
        }
        let previous = self.warned.get(key).copied();
        let percent = battery.percent;
        if percent <= CRITICAL && previous != Some(Level::Critical) {
            self.warned.insert(key.to_string(), Level::Critical);
            return Some(Alert { title: format!("{name} is almost out of battery"), body: format!("{percent}% left. Plug it in to keep it running.") });
        }
        if percent <= LOW && previous.is_none() {
            self.warned.insert(key.to_string(), Level::Low);
            return Some(Alert { title: format!("{name} battery is low"), body: format!("{percent}% left. Time to charge it.") });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(percent: u8, charging: bool) -> Battery {
        Battery { percent, charging, millivolts: None }
    }

    #[test]
    fn warns_once_per_level_and_rearms_after_charging() {
        let mut a = BatteryAlerts::default();
        assert_eq!(a.check("m", "Mouse", at(40, false)), None);
        assert!(a.check("m", "Mouse", at(20, false)).unwrap().title.contains("low"));
        assert_eq!(a.check("m", "Mouse", at(18, false)), None, "already warned about this level");
        assert!(a.check("m", "Mouse", at(5, false)).unwrap().title.contains("almost out"));
        assert_eq!(a.check("m", "Mouse", at(3, false)), None);
        assert_eq!(a.check("m", "Mouse", at(3, true)), None, "charging resets the warnings");
        assert!(a.check("m", "Mouse", at(19, false)).is_some(), "unplugged low again: warn again");
    }

    #[test]
    fn hysteresis_avoids_repeats_around_the_threshold() {
        let mut a = BatteryAlerts::default();
        assert!(a.check("k", "Keyboard", at(20, false)).is_some());
        assert_eq!(a.check("k", "Keyboard", at(21, false)), None);
        assert_eq!(a.check("k", "Keyboard", at(20, false)), None, "a reading bouncing at 20/21 must not warn twice");
        assert_eq!(a.check("k", "Keyboard", at(30, false)), None);
        assert!(a.check("k", "Keyboard", at(20, false)).is_some(), "back above 25 % re-arms");
    }

    #[test]
    fn first_reading_already_critical_gives_the_critical_warning() {
        let mut a = BatteryAlerts::default();
        let alert = a.check("x", "Headset", at(4, false)).unwrap();
        assert!(alert.body.starts_with("4%"));
        assert_eq!(a.check("x", "Headset", at(4, false)), None);
        assert!(a.check("y", "Mouse", at(4, false)).is_some(), "devices are tracked separately");
    }
}
