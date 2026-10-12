//! Tauri shell: hosts the engine, exposes commands to the UI, forwards engine
//! state as events, keeps a tray icon, and remembers the current scene.
//!
//! Footprint rules: the window is disposable. Closing it destroys the WebView
//! (the only heavy part) while the engine keeps lighting from the tray; preview
//! frames are only produced while a window is visible.

mod openrgb;
mod sidecar;
mod updates;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lumen_core::engine::{self, Command, EngineConfig, EngineHandle, FrameSet, Snapshot};
use lumen_core::{Layer, Layout, Profile, ProfileStore, Scene};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tokio::sync::broadcast::error::RecvError;
use tracing_subscriber::EnvFilter;

const TRAY_ID: &str = "lumen-tray";
const DEFAULT_FPS: u32 = 60;

/// Small, user-editable preferences; the scene itself is autosaved separately.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    fps: Option<u32>,
    /// Daily background update check (default on).
    #[serde(default)]
    auto_update: Option<bool>,
}

impl Settings {
    fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &Path) {
        if let Err(e) = std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(dir.join("settings.json"), serde_json::to_string_pretty(self).unwrap_or_default()))
        {
            tracing::warn!(%e, "could not save settings");
        }
    }
}

struct AppState {
    engine: EngineHandle,
    profiles: ProfileStore,
    openrgb: openrgb::Manager,
    settings: Mutex<Settings>,
    data_dir: PathBuf,
}

/// Preview frame as emitted to the UI: flat RGB bytes per device.
#[derive(Clone, Serialize)]
struct FramePayload {
    t: f64,
    devices: Vec<FrameDevice>,
}

#[derive(Clone, Serialize)]
struct FrameDevice {
    id: u32,
    rgb: Vec<u8>,
}

impl From<&FrameSet> for FramePayload {
    fn from(f: &FrameSet) -> Self {
        let devices = f
            .device_ids
            .iter()
            .zip(&f.frames)
            .map(|(&id, colors)| FrameDevice { id, rgb: colors.iter().flat_map(|c| [c.r, c.g, c.b]).collect() })
            .collect();
        FramePayload { t: f.t, devices }
    }
}

type CmdResult<T = ()> = Result<T, String>;

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    (*state.engine.snapshot()).clone()
}

#[tauri::command]
async fn set_layers(state: State<'_, AppState>, layers: Vec<Layer>) -> CmdResult {
    send(&state.engine, Command::SetLayers(layers)).await
}

#[tauri::command]
async fn set_layout(state: State<'_, AppState>, layout: Layout) -> CmdResult {
    send(&state.engine, Command::SetLayout(layout)).await
}

#[tauri::command]
async fn set_brightness(state: State<'_, AppState>, brightness: f32) -> CmdResult {
    send(&state.engine, Command::SetBrightness(brightness)).await
}

#[tauri::command]
async fn set_scene(state: State<'_, AppState>, scene: Scene) -> CmdResult {
    send(&state.engine, Command::SetScene(scene)).await
}

#[tauri::command]
async fn set_paused(state: State<'_, AppState>, paused: bool) -> CmdResult {
    send(&state.engine, if paused { Command::Pause } else { Command::Resume }).await
}

#[tauri::command]
async fn set_fps(state: State<'_, AppState>, fps: u32) -> CmdResult {
    let fps = fps.clamp(15, 120);
    send(&state.engine, Command::SetFps(fps)).await?;
    let mut settings = state.settings.lock().unwrap();
    settings.fps = Some(fps);
    settings.save(&state.data_dir);
    Ok(())
}

#[tauri::command]
async fn refresh_devices(state: State<'_, AppState>) -> CmdResult {
    send(&state.engine, Command::RefreshDevices).await
}

#[tauri::command]
async fn save_to_device(state: State<'_, AppState>, device_id: u32, effect: sidecar::OnboardEffect) -> CmdResult {
    let device = state
        .engine
        .snapshot()
        .devices
        .iter()
        .find(|d| d.id == device_id)
        .cloned()
        .ok_or_else(|| "device is no longer connected".to_string())?;
    sidecar::save_to_device(&state.engine, &device, effect).await
}

#[tauri::command]
fn list_profiles(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    state.profiles.list().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_profile(state: State<'_, AppState>, name: String) -> CmdResult<Vec<String>> {
    let scene = state.engine.snapshot().scene.clone();
    state.profiles.save(&Profile { name, scene }).map_err(|e| e.to_string())?;
    state.profiles.list().map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_profile(state: State<'_, AppState>, name: String) -> CmdResult {
    let profile = state.profiles.load(&name).map_err(|e| e.to_string())?;
    send(&state.engine, Command::SetScene(profile.scene)).await
}

#[tauri::command]
fn delete_profile(state: State<'_, AppState>, name: String) -> CmdResult<Vec<String>> {
    state.profiles.delete(&name).map_err(|e| e.to_string())?;
    state.profiles.list().map_err(|e| e.to_string())
}

#[tauri::command]
fn openrgb_status(state: State<'_, AppState>) -> openrgb::Status {
    state.openrgb.status()
}

#[tauri::command]
async fn start_openrgb(state: State<'_, AppState>) -> CmdResult<openrgb::Status> {
    state.openrgb.ensure_running().await
}

/// App-level preferences and version info for the Settings panel.
#[derive(Clone, Serialize)]
struct AppSettings {
    version: String,
    auto_update: bool,
    /// Update found by the last check, if any.
    update: Option<updates::UpdateInfo>,
}

#[tauri::command]
fn app_settings(app: AppHandle, state: State<'_, AppState>) -> AppSettings {
    AppSettings {
        version: app.package_info().version.to_string(),
        auto_update: state.settings.lock().unwrap().auto_update.unwrap_or(true),
        update: app.state::<updates::UpdateState>().available(),
    }
}

#[tauri::command]
fn set_auto_update(state: State<'_, AppState>, enabled: bool) {
    let mut settings = state.settings.lock().unwrap();
    settings.auto_update = Some(enabled);
    settings.save(&state.data_dir);
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> CmdResult<Option<updates::UpdateInfo>> {
    updates::check(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> CmdResult {
    updates::install(&app).await
}

async fn send(engine: &EngineHandle, cmd: Command) -> CmdResult {
    if engine.send(cmd).await { Ok(()) } else { Err("engine is not running".into()) }
}

fn data_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("Lumen"));
    migrate_legacy_data(&dir);
    dir
}

/// Early builds used the identifier `dev.lumen.app`; carry their data over once.
fn migrate_legacy_data(dir: &Path) {
    if dir.join("scene.json").exists() {
        return;
    }
    let Some(old) = dir.parent().map(|p| p.join("dev.lumen.app")) else { return };
    if !old.join("scene.json").exists() {
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    for name in ["scene.json", "settings.json"] {
        let _ = std::fs::copy(old.join(name), dir.join(name));
    }
    if let Ok(entries) = std::fs::read_dir(old.join("profiles")) {
        let _ = std::fs::create_dir_all(dir.join("profiles"));
        for entry in entries.flatten() {
            let _ = std::fs::copy(entry.path(), dir.join("profiles").join(entry.file_name()));
        }
    }
    tracing::info!(from = %old.display(), to = %dir.display(), "migrated data from the old identifier");
}

fn load_last_scene(dir: &Path) -> Scene {
    match std::fs::read_to_string(dir.join("scene.json")) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(%e, "scene.json unreadable, starting fresh");
            Scene::default()
        }),
        Err(_) => Scene::default(),
    }
}

/// Persist the current scene whenever it changes, debounced so slider drags
/// don't hammer the disk.
async fn autosave_scene(engine: EngineHandle, dir: PathBuf) {
    let mut rx = engine.watch_snapshot();
    let mut last_saved: Option<Scene> = None;
    loop {
        if rx.changed().await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                changed = rx.changed() => if changed.is_err() { return },
                _ = tokio::time::sleep(Duration::from_millis(600)) => break,
            }
        }
        let scene = rx.borrow().scene.clone();
        if last_saved.as_ref() == Some(&scene) {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(dir.join("scene.json"), serde_json::to_string_pretty(&scene).unwrap_or_default()))
        {
            tracing::warn!(%e, "could not save scene");
        }
        last_saved = Some(scene);
    }
}

/// Forward snapshots to the UI and keep the tray tooltip (with battery levels) current.
async fn forward_snapshots(handle: AppHandle, engine: EngineHandle) {
    let mut rx = engine.watch_snapshot();
    let mut last_tip = String::new();
    while rx.changed().await.is_ok() {
        let snap: Arc<Snapshot> = rx.borrow().clone();
        let _ = handle.emit("snapshot", &*snap);
        let tip = tray_tooltip(&snap, handle.state::<updates::UpdateState>().available());
        if tip != last_tip {
            if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                let _ = tray.set_tooltip(Some(&tip));
            }
            last_tip = tip;
        }
    }
}

fn tray_tooltip(snap: &Snapshot, update: Option<updates::UpdateInfo>) -> String {
    let mut parts = vec![if snap.connected { "Lumen".to_string() } else { "Lumen — waiting for OpenRGB".to_string() }];
    if let Some(u) = update {
        parts.push(format!("Update {} available — open Lumen to install", u.version));
    }
    for d in &snap.devices {
        if let Some(b) = d.battery {
            parts.push(format!("{} {}%{}", d.display_name, b.percent, if b.charging { " (charging)" } else { "" }));
        }
    }
    parts.join("\n")
}

/// Forward preview frames while a window is visible; otherwise drop the
/// subscription so the engine does not even build preview frames.
async fn forward_frames(handle: AppHandle, engine: EngineHandle) {
    let mut rx = None;
    loop {
        let visible = handle.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
        if !visible {
            rx = None;
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        let frames = rx.get_or_insert_with(|| engine.subscribe_frames());
        match tokio::time::timeout(Duration::from_millis(500), frames.recv()).await {
            Ok(Ok(frame)) => {
                let _ = handle.emit("frame", FramePayload::from(&*frame));
            }
            Ok(Err(RecvError::Lagged(_))) | Err(_) => continue,
            Ok(Err(RecvError::Closed)) => break,
        }
    }
}

/// Show the main window, recreating it from the config if it was closed.
fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let Some(cfg) = app.config().app.windows.first().cloned() else { return };
    match tauri::WebviewWindowBuilder::from_config(app, &cfg).and_then(|b| b.build()) {
        Ok(_) => tracing::debug!("window recreated"),
        Err(e) => tracing::warn!(%e, "could not recreate window"),
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Lumen", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Lumen", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app.default_window_icon().cloned().ok_or(tauri::Error::WindowNotFound)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Lumen")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,lumen=debug,lumen_core=debug,openrgb_sdk=info")),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let dir = data_dir(&handle);
            let scene = load_last_scene(&dir);
            let settings = Settings::load(&dir);
            let config = EngineConfig { fps: settings.fps.unwrap_or(DEFAULT_FPS).clamp(15, 120), ..EngineConfig::default() };

            // The engine spawns Tokio tasks, so create it on Tauri's runtime.
            let engine = tauri::async_runtime::block_on(async { engine::spawn(config, scene) });

            // Register state before any task can call `handle.state()`.
            app.manage(AppState {
                engine: engine.clone(),
                profiles: ProfileStore::new(dir.join("profiles")),
                openrgb: openrgb::Manager::new(handle.path().resource_dir().ok(), dir.clone()),
                settings: Mutex::new(settings),
                data_dir: dir.clone(),
            });
            app.manage(updates::UpdateState::default());

            // The window is configured hidden; show it unless we were launched
            // at login (autostart passes --hidden) so lighting starts silently.
            if !std::env::args().any(|a| a == "--hidden") {
                show_main_window(&handle);
            }

            // Bring up OpenRGB's SDK server in the background; the engine
            // reconnects on its own once the port opens.
            {
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state: State<AppState> = handle.state();
                    match state.openrgb.ensure_running().await {
                        Ok(status) => tracing::info!(?status, "OpenRGB ready"),
                        Err(e) => tracing::warn!(%e, "OpenRGB unavailable"),
                    }
                    let _ = handle.emit("openrgb_status", state.openrgb.status());
                });
            }

            tauri::async_runtime::spawn(forward_snapshots(handle.clone(), engine.clone()));
            tauri::async_runtime::spawn(forward_frames(handle.clone(), engine.clone()));
            tauri::async_runtime::spawn(autosave_scene(engine.clone(), dir));
            tauri::async_runtime::spawn(sidecar::run(engine.clone()));
            {
                let enabled = {
                    let h = handle.clone();
                    move || h.state::<AppState>().settings.lock().unwrap().auto_update.unwrap_or(true)
                };
                tauri::async_runtime::spawn(updates::auto_check(handle.clone(), enabled));
            }

            setup_tray(&handle)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_layers,
            set_layout,
            set_brightness,
            set_scene,
            set_paused,
            set_fps,
            refresh_devices,
            save_to_device,
            list_profiles,
            save_profile,
            load_profile,
            delete_profile,
            openrgb_status,
            start_openrgb,
            app_settings,
            set_auto_update,
            check_for_update,
            install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lumen");

    app.run(|app, event| match event {
        // Closing the last window must not end the app: lighting keeps running
        // from the tray, and the window is rebuilt on demand.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        // Take the OpenRGB copy we started down with us.
        RunEvent::Exit => app.state::<AppState>().openrgb.stop(),
        _ => {}
    });
}
