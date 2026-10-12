//! Optional global keyboard shortcuts (Settings → Keyboard shortcuts). All of
//! them use Ctrl+Alt+Shift so they stay out of other apps' way:
//! L switches the lights, ↑/↓ change brightness, →/← step through profiles.

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

use crate::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    ToggleLights,
    Brighter,
    Dimmer,
    NextProfile,
    PreviousProfile,
}

fn bindings() -> [(Shortcut, Action); 5] {
    let mods = Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT);
    [
        (Shortcut::new(mods, Code::KeyL), Action::ToggleLights),
        (Shortcut::new(mods, Code::ArrowUp), Action::Brighter),
        (Shortcut::new(mods, Code::ArrowDown), Action::Dimmer),
        (Shortcut::new(mods, Code::ArrowRight), Action::NextProfile),
        (Shortcut::new(mods, Code::ArrowLeft), Action::PreviousProfile),
    ]
}

/// Register (or drop) the shortcuts. Fails when another app already owns one.
pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    shortcuts.unregister_all().map_err(|e| e.to_string())?;
    if !enabled {
        return Ok(());
    }
    for (shortcut, _) in bindings() {
        if let Err(e) = shortcuts.register(shortcut) {
            let _ = shortcuts.unregister_all();
            return Err(format!("Another app already uses {}: {e}", describe(&shortcut)));
        }
    }
    Ok(())
}

fn describe(shortcut: &Shortcut) -> String {
    let key = match shortcut.key {
        Code::KeyL => "L",
        Code::ArrowUp => "↑",
        Code::ArrowDown => "↓",
        Code::ArrowRight => "→",
        Code::ArrowLeft => "←",
        _ => "?",
    };
    format!("Ctrl+Alt+Shift+{key}")
}

/// The plugin's handler for every registered shortcut.
pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let Some(action) = bindings().into_iter().find(|(s, _)| s.mods == shortcut.mods && s.key == shortcut.key).map(|(_, a)| a) else { return };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = match action {
            Action::ToggleLights => {
                let off = !app.state::<AppState>().lights.get().user_off;
                crate::set_user_lights_off(&app, off).await
            }
            Action::Brighter | Action::Dimmer => {
                let now = app.state::<AppState>().engine.snapshot().scene.brightness;
                let step = if action == Action::Brighter { 0.1 } else { -0.1 };
                // Snap to tenths so repeated presses land on round numbers.
                crate::set_brightness_from_outside(&app, ((now + step) * 10.0).round() / 10.0).await
            }
            Action::NextProfile | Action::PreviousProfile => step_profile(&app, if action == Action::NextProfile { 1 } else { -1 }).await,
        };
        if let Err(e) = result {
            tracing::warn!(?action, %e, "shortcut failed");
        }
    });
}

async fn step_profile(app: &AppHandle, delta: isize) -> Result<(), String> {
    let state = app.state::<AppState>();
    let profiles = state.profiles.list().map_err(|e| e.to_string())?;
    if profiles.is_empty() {
        return Ok(());
    }
    let current = state.current_profile.lock().unwrap().clone();
    let next = next_index(profiles.len(), current.and_then(|c| profiles.iter().position(|p| *p == c)), delta);
    crate::apply_profile(app, &profiles[next]).await
}

/// Index after (or before) `current` in a list of `len`, wrapping; from nothing, the first (or last).
fn next_index(len: usize, current: Option<usize>, delta: isize) -> usize {
    match current {
        Some(i) => (i as isize + delta).rem_euclid(len as isize) as usize,
        None if delta >= 0 => 0,
        None => len - 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_stepping_wraps_both_ways() {
        assert_eq!(next_index(3, Some(0), 1), 1);
        assert_eq!(next_index(3, Some(2), 1), 0);
        assert_eq!(next_index(3, Some(0), -1), 2);
        assert_eq!(next_index(3, None, 1), 0);
        assert_eq!(next_index(3, None, -1), 2);
        assert_eq!(next_index(1, Some(0), 1), 0);
    }

    #[test]
    fn every_binding_has_a_name_and_they_differ() {
        let b = bindings();
        for (s, _) in &b {
            assert!(!describe(s).ends_with('?'));
        }
        for i in 0..b.len() {
            for j in i + 1..b.len() {
                assert_ne!(b[i].0.key, b[j].0.key);
            }
        }
    }
}
