//! Decides when the lights are off: the user's own switch, plus whatever the
//! PC is doing (sleeping, screen off, locked, idle) as far as the user asked
//! Lumen to follow it. The engine only ever hears the outcome.

use std::sync::Mutex;

use lumen_core::Blackout;
use serde::{Deserialize, Serialize};

use crate::power::PowerEvent;

/// Which PC states turn the lights off (Settings → Lights).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerSettings {
    /// While the PC sleeps, and when it shuts down.
    pub sleep: bool,
    /// While the display is off.
    pub display_off: bool,
    /// While the session is locked.
    pub lock: bool,
    /// After this many minutes without keyboard or mouse input; 0 = never.
    pub idle_minutes: u32,
}

impl Default for PowerSettings {
    fn default() -> Self {
        Self { sleep: true, display_off: true, lock: false, idle_minutes: 0 }
    }
}

/// What is known about the PC and the user's wishes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Policy {
    pub settings: PowerSettings,
    /// The lights switch in the top bar, tray or shortcut.
    pub user_off: bool,
    pub asleep: bool,
    pub display_off: bool,
    pub locked: bool,
    pub idle: bool,
}

impl Policy {
    /// Why the lights should be off right now, if they should.
    pub fn blackout(&self) -> Option<Blackout> {
        let s = self.settings;
        if self.user_off {
            Some(Blackout::User)
        } else if self.asleep && s.sleep {
            Some(Blackout::Sleep)
        } else if self.locked && s.lock {
            Some(Blackout::Lock)
        } else if self.display_off && s.display_off {
            Some(Blackout::Display)
        } else if self.idle && s.idle_minutes > 0 {
            Some(Blackout::Idle)
        } else {
            None
        }
    }

    pub fn apply(&mut self, event: PowerEvent) {
        match event {
            PowerEvent::Suspend | PowerEvent::Shutdown => self.asleep = true,
            PowerEvent::ShutdownCancelled => self.asleep = false,
            PowerEvent::Resume => {
                self.asleep = false;
                self.idle = false;
            }
            PowerEvent::Display(on) => self.display_off = !on,
            PowerEvent::Lock(locked) => self.locked = locked,
        }
    }

    /// Feed the seconds since the last keyboard or mouse input.
    pub fn set_idle_seconds(&mut self, seconds: u64) {
        let limit = u64::from(self.settings.idle_minutes) * 60;
        self.idle = limit > 0 && seconds >= limit;
    }
}

/// The policy as shared state; every change returns the resulting blackout,
/// which the caller hands to the engine (it ignores repeats).
pub struct Lights {
    policy: Mutex<Policy>,
}

impl Lights {
    pub fn new(settings: PowerSettings, user_off: bool) -> Self {
        Self { policy: Mutex::new(Policy { settings, user_off, ..Policy::default() }) }
    }

    pub fn update(&self, change: impl FnOnce(&mut Policy)) -> Option<Blackout> {
        let mut policy = self.policy.lock().unwrap();
        change(&mut policy);
        policy.blackout()
    }

    pub fn get(&self) -> Policy {
        *self.policy.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_sleep_and_the_display_but_not_the_lock() {
        let mut p = Policy::default();
        assert_eq!(p.blackout(), None);
        p.apply(PowerEvent::Lock(true));
        assert_eq!(p.blackout(), None, "locking alone keeps the lights on by default");
        p.apply(PowerEvent::Display(false));
        assert_eq!(p.blackout(), Some(Blackout::Display));
        p.apply(PowerEvent::Display(true));
        assert_eq!(p.blackout(), None);
        p.apply(PowerEvent::Suspend);
        assert_eq!(p.blackout(), Some(Blackout::Sleep));
        p.apply(PowerEvent::Resume);
        assert_eq!(p.blackout(), None);
        p.apply(PowerEvent::Shutdown);
        assert_eq!(p.blackout(), Some(Blackout::Sleep));
        p.apply(PowerEvent::ShutdownCancelled);
        assert_eq!(p.blackout(), None, "a cancelled shutdown brings the lights back");
    }

    #[test]
    fn the_users_switch_wins_and_reasons_can_be_turned_off() {
        let mut p = Policy { settings: PowerSettings { sleep: false, display_off: false, lock: true, idle_minutes: 0 }, ..Policy::default() };
        p.apply(PowerEvent::Suspend);
        p.apply(PowerEvent::Display(false));
        assert_eq!(p.blackout(), None, "sleep and display are switched off in the settings");
        p.apply(PowerEvent::Lock(true));
        assert_eq!(p.blackout(), Some(Blackout::Lock));
        p.user_off = true;
        assert_eq!(p.blackout(), Some(Blackout::User));
        p.apply(PowerEvent::Lock(false));
        assert_eq!(p.blackout(), Some(Blackout::User), "unlocking does not undo the user's switch");
    }

    #[test]
    fn idle_needs_a_limit_and_is_cleared_by_input_or_waking() {
        let mut p = Policy::default();
        p.set_idle_seconds(10_000);
        assert_eq!(p.blackout(), None, "idle detection is off by default");
        p.settings.idle_minutes = 10;
        p.set_idle_seconds(599);
        assert_eq!(p.blackout(), None);
        p.set_idle_seconds(600);
        assert_eq!(p.blackout(), Some(Blackout::Idle));
        p.set_idle_seconds(2);
        assert_eq!(p.blackout(), None);
        p.set_idle_seconds(900);
        p.apply(PowerEvent::Resume);
        assert_eq!(p.blackout(), None);
    }
}
