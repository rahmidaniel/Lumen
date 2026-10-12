//! The tray icon and its menu: show the window, lights on/off, brightness,
//! profiles, quit. The menu is rebuilt whenever one of those changes, so its
//! check marks always match the app.

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::AppState;

pub const TRAY_ID: &str = "lumen-tray";
pub const BRIGHTNESS_STEPS: [u8; 4] = [100, 75, 50, 25];

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let icon = app.default_window_icon().cloned().ok_or(tauri::Error::WindowNotFound)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Lumen")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuild the menu from the current state.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => tracing::warn!(%e, "could not rebuild the tray menu"),
    }
}

/// The brightness step (if any) the current brightness sits on.
pub fn brightness_step(brightness: f32) -> Option<u8> {
    let pct = (brightness * 100.0).round() as u8;
    BRIGHTNESS_STEPS.contains(&pct).then_some(pct)
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let lights_off = state.lights.get().user_off;
    let step = brightness_step(state.engine.snapshot().scene.brightness);
    let profiles = state.profiles.list().unwrap_or_default();
    let current = state.current_profile.lock().unwrap().clone();

    let show = MenuItem::with_id(app, "show", "Show Lumen", true, None::<&str>)?;
    let lights = CheckMenuItem::with_id(app, "lights", "Lights off", true, lights_off, None::<&str>)?;

    let levels = BRIGHTNESS_STEPS
        .iter()
        .map(|&b| CheckMenuItem::with_id(app, format!("brightness:{b}"), format!("{b}%"), true, step == Some(b), None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;
    let level_refs: Vec<&dyn IsMenuItem<Wry>> = levels.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let brightness = Submenu::with_items(app, "Brightness", true, &level_refs)?;

    let profile_items = profiles
        .iter()
        .map(|name| CheckMenuItem::with_id(app, format!("profile:{name}"), name, true, current.as_deref() == Some(name), None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;
    let none = MenuItem::with_id(app, "no-profiles", "No saved profiles", false, None::<&str>)?;
    let profile_refs: Vec<&dyn IsMenuItem<Wry>> =
        if profile_items.is_empty() { vec![&none] } else { profile_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect() };
    let profiles_menu = Submenu::with_items(app, "Profiles", true, &profile_refs)?;

    let separator = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Lumen", true, None::<&str>)?;
    Menu::with_items(app, &[&show, &separator, &lights, &brightness, &profiles_menu, &separator2, &quit])
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => crate::show_main_window(app),
        "quit" => app.exit(0),
        "lights" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let off = !app.state::<AppState>().lights.get().user_off;
                if let Err(e) = crate::set_user_lights_off(&app, off).await {
                    tracing::warn!(%e, "could not switch the lights");
                }
            });
        }
        _ => {
            if let Some(level) = id.strip_prefix("brightness:").and_then(|v| v.parse::<u8>().ok()) {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::set_brightness_from_outside(&app, f32::from(level) / 100.0).await {
                        tracing::warn!(%e, "could not set the brightness");
                    }
                });
            } else if let Some(name) = id.strip_prefix("profile:") {
                let (app, name) = (app.clone(), name.to_string());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::apply_profile(&app, &name).await {
                        tracing::warn!(%e, profile = %name, "could not load the profile");
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_steps_match_rounded_levels_only() {
        assert_eq!(brightness_step(1.0), Some(100));
        assert_eq!(brightness_step(0.7501), Some(75));
        assert_eq!(brightness_step(0.25), Some(25));
        assert_eq!(brightness_step(0.6), None);
        assert_eq!(brightness_step(0.0), None);
    }
}
