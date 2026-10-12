//! Tauri shell: hosts the engine, exposes commands to the UI, forwards engine
//! state as events, keeps a tray icon, remembers the current scene, and makes
//! the lights follow the PC (sleep, screen, lock, idle).
//!
//! Footprint rules: the window is disposable. Closing it destroys the WebView
//! (the only heavy part) while the engine keeps lighting from the tray; preview
//! frames are only produced while a window is visible.

mod alerts;
mod conflicts;
mod diagnostics;
mod lights;
mod openrgb;
mod power;
mod shortcuts;
mod sidecar;
mod tray;
mod updates;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lumen_core::engine::{self, Command, EngineConfig, EngineHandle, FrameSet, Snapshot};
use lumen_core::{Blackout, Layer, Layout, Profile, ProfileStore, Scene};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::broadcast::error::RecvError;

use crate::lights::{Lights, PowerSettings};
use crate::power::PowerEvent;

const DEFAULT_FPS: u32 = 60;

/// Small, user-editable preferences; the scene itself is autosaved separately.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    fps: Option<u32>,
    /// Daily background update check (default on).
    #[serde(default)]
    auto_update: Option<bool>,
    /// The lights switch: off stays off across restarts.
    #[serde(default)]
    lights_off: bool,
    /// Which PC states turn the lights off.
    #[serde(default)]
    power: PowerSettings,
    /// Low-battery notifications (default on).
    #[serde(default)]
    battery_alerts: Option<bool>,
    /// Global keyboard shortcuts (default off).
    #[serde(default)]
    shortcuts: bool,
    /// Vendor apps the user does not want to hear about again.
    #[serde(default)]
    dismissed_conflicts: Vec<String>,
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
    lights: Lights,
    /// Lighting apps found running, dismissed ones included.
    conflicts: Mutex<Vec<conflicts::Conflict>>,
    /// The profile loaded last (tray check mark, next/previous shortcuts).
    current_profile: Mutex<Option<String>>,
    /// Where Lumen's log files go, when file logging could be set up.
    log_dir: Option<PathBuf>,
}

impl AppState {
    fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings.lock().unwrap();
        change(&mut settings);
        settings.save(&self.data_dir);
    }
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
async fn set_lights_off(app: AppHandle, off: bool) -> CmdResult {
    set_user_lights_off(&app, off).await
}

#[tauri::command]
async fn set_fps(state: State<'_, AppState>, fps: u32) -> CmdResult {
    let fps = fps.clamp(15, 120);
    send(&state.engine, Command::SetFps(fps)).await?;
    state.update_settings(|s| s.fps = Some(fps));
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
fn save_profile(app: AppHandle, state: State<'_, AppState>, name: String) -> CmdResult<Vec<String>> {
    let scene = state.engine.snapshot().scene.clone();
    state.profiles.save(&Profile { name: name.clone(), scene }).map_err(|e| e.to_string())?;
    *state.current_profile.lock().unwrap() = Some(name);
    tray::refresh(&app);
    state.profiles.list().map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_profile(app: AppHandle, name: String) -> CmdResult {
    apply_profile(&app, &name).await
}

#[tauri::command]
fn delete_profile(app: AppHandle, state: State<'_, AppState>, name: String) -> CmdResult<Vec<String>> {
    state.profiles.delete(&name).map_err(|e| e.to_string())?;
    {
        let mut current = state.current_profile.lock().unwrap();
        if current.as_deref() == Some(name.as_str()) {
            *current = None;
        }
    }
    tray::refresh(&app);
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

/// Restart OpenRGB: quietly when Lumen runs it, through an elevated prompt
/// when it is the Windows service (or ignores the stop request). The engine
/// reconnects on its own.
#[tauri::command]
async fn restart_openrgb(state: State<'_, AppState>) -> CmdResult<openrgb::Status> {
    state.openrgb.restart(true).await
}

/// Windows: hand OpenRGB over to Lumen (retire OpenRGB's service, install
/// Lumen's own), then start it. Also updates an existing setup. One admin prompt.
#[tauri::command]
async fn adopt_openrgb(state: State<'_, AppState>) -> CmdResult<openrgb::Status> {
    state.openrgb.adopt().await
}

/// Windows: remove Lumen's service and give OpenRGB back. One admin prompt.
#[tauri::command]
async fn release_openrgb(state: State<'_, AppState>) -> CmdResult<openrgb::Status> {
    state.openrgb.release().await
}

/// App-level preferences and version info for the Settings panel.
#[derive(Clone, Serialize)]
struct AppSettings {
    version: String,
    auto_update: bool,
    /// Update found by the last check, if any.
    update: Option<updates::UpdateInfo>,
    lights_off: bool,
    power: PowerSettings,
    /// This platform reports sleep, display and lock changes.
    power_supported: bool,
    battery_alerts: bool,
    shortcuts: bool,
    log_dir: Option<String>,
}

#[tauri::command]
fn app_settings(app: AppHandle, state: State<'_, AppState>) -> AppSettings {
    let settings = state.settings.lock().unwrap().clone();
    AppSettings {
        version: app.package_info().version.to_string(),
        auto_update: settings.auto_update.unwrap_or(true),
        update: app.state::<updates::UpdateState>().available(),
        lights_off: settings.lights_off,
        power: settings.power,
        power_supported: power::SUPPORTED,
        battery_alerts: settings.battery_alerts.unwrap_or(true),
        shortcuts: settings.shortcuts,
        log_dir: state.log_dir.as_ref().map(|d| d.display().to_string()),
    }
}

#[tauri::command]
fn set_auto_update(state: State<'_, AppState>, enabled: bool) {
    state.update_settings(|s| s.auto_update = Some(enabled));
}

#[tauri::command]
async fn set_power_settings(state: State<'_, AppState>, power: PowerSettings) -> CmdResult {
    state.update_settings(|s| s.power = power);
    let blackout = state.lights.update(|p| {
        p.settings = power;
        if power.idle_minutes == 0 {
            p.idle = false;
        }
    });
    send(&state.engine, Command::SetBlackout(blackout)).await
}

#[tauri::command]
fn set_battery_alerts(state: State<'_, AppState>, enabled: bool) {
    state.update_settings(|s| s.battery_alerts = Some(enabled));
}

#[tauri::command]
fn set_shortcuts(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> CmdResult {
    let result = shortcuts::apply(&app, enabled);
    state.update_settings(|s| s.shortcuts = enabled && result.is_ok());
    result
}

/// Running lighting apps the user has not dismissed.
fn visible_conflicts(state: &AppState) -> Vec<conflicts::Conflict> {
    let dismissed = state.settings.lock().unwrap().dismissed_conflicts.clone();
    state.conflicts.lock().unwrap().iter().filter(|c| !dismissed.iter().any(|d| d == c.id)).cloned().collect()
}

#[tauri::command]
fn get_conflicts(state: State<'_, AppState>) -> Vec<conflicts::Conflict> {
    visible_conflicts(&state)
}

#[tauri::command]
fn dismiss_conflict(state: State<'_, AppState>, id: String) -> Vec<conflicts::Conflict> {
    state.update_settings(|s| {
        if !s.dismissed_conflicts.contains(&id) {
            s.dismissed_conflicts.push(id);
        }
    });
    visible_conflicts(&state)
}

#[tauri::command]
fn diagnostics(app: AppHandle) -> String {
    diagnostics::report(&app)
}

#[tauri::command]
fn open_log_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult {
    let dir = state.log_dir.clone().ok_or_else(|| "Lumen is not writing a log file.".to_string())?;
    app.opener().open_path(dir.display().to_string(), None::<&str>).map_err(|e| e.to_string())
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
    if engine.send(cmd).await {
        Ok(())
    } else {
        Err("engine is not running".into())
    }
}

/// The lights switch (top bar, tray, shortcut). Remembered across restarts.
async fn set_user_lights_off(app: &AppHandle, off: bool) -> CmdResult {
    let state = app.state::<AppState>();
    state.update_settings(|s| s.lights_off = off);
    let blackout = state.lights.update(|p| p.user_off = off);
    send(&state.engine, Command::SetBlackout(blackout)).await?;
    tray::refresh(app);
    Ok(())
}

/// Brightness from the tray or a shortcut (the UI hears it through the snapshot).
async fn set_brightness_from_outside(app: &AppHandle, brightness: f32) -> CmdResult {
    send(&app.state::<AppState>().engine, Command::SetBrightness(brightness.clamp(0.0, 1.0))).await
}

async fn apply_profile(app: &AppHandle, name: &str) -> CmdResult {
    let state = app.state::<AppState>();
    let profile = state.profiles.load(name).map_err(|e| e.to_string())?;
    send(&state.engine, Command::SetScene(profile.scene)).await?;
    *state.current_profile.lock().unwrap() = Some(name.to_string());
    tray::refresh(app);
    Ok(())
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

/// Forward snapshots to the UI, keep the tray tooltip (with battery levels)
/// current, and the tray menu's check marks in step with brightness and lights.
async fn forward_snapshots(handle: AppHandle, engine: EngineHandle) {
    let mut rx = engine.watch_snapshot();
    let mut last_tip = String::new();
    let mut last_menu: Option<(Option<Blackout>, Option<u8>)> = None;
    while rx.changed().await.is_ok() {
        let snap: Arc<Snapshot> = rx.borrow().clone();
        let _ = handle.emit("snapshot", &*snap);
        let tip = tray_tooltip(&snap, handle.state::<updates::UpdateState>().available());
        if tip != last_tip {
            if let Some(tray) = handle.tray_by_id(tray::TRAY_ID) {
                let _ = tray.set_tooltip(Some(&tip));
            }
            last_tip = tip;
        }
        let menu = (snap.blackout, tray::brightness_step(snap.scene.brightness));
        if last_menu.is_some_and(|m| m != menu) {
            tray::refresh(&handle);
        }
        last_menu = Some(menu);
    }
}

fn tray_tooltip(snap: &Snapshot, update: Option<updates::UpdateInfo>) -> String {
    let mut parts = vec![if snap.connected { "Lumen".to_string() } else { "Lumen — waiting for OpenRGB".to_string() }];
    if snap.blackout.is_some() {
        parts.push("Lights off".to_string());
    }
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

/// Keep an OpenRGB running while that is Lumen's job: restart it when it
/// stops answering (after ten seconds, at most every two minutes), and start
/// it again when it went away altogether (after five seconds, at most every
/// thirty). An OpenRGB that belongs to someone else is left alone.
async fn supervise_openrgb(handle: AppHandle, engine: EngineHandle) {
    let mut rx = engine.watch_snapshot();
    let mut unresponsive_since: Option<std::time::Instant> = None;
    let mut gone_since: Option<std::time::Instant> = None;
    let mut last_restart: Option<std::time::Instant> = None;
    let mut last_start: Option<std::time::Instant> = None;
    loop {
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
        let (connected, unresponsive) = {
            let snap = rx.borrow();
            (snap.connected, snap.problem.is_some())
        };
        if connected {
            unresponsive_since = None;
            gone_since = None;
            continue;
        }
        let state: State<AppState> = handle.state();
        let reachable = tokio::task::spawn_blocking(openrgb::server_reachable).await.unwrap_or(false);
        let recently = |at: Option<std::time::Instant>, gap: u64| at.is_some_and(|t| t.elapsed() < Duration::from_secs(gap));

        if reachable && unresponsive {
            gone_since = None;
            let since = *unresponsive_since.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() < Duration::from_secs(10) || recently(last_restart, 120) || !state.openrgb.can_restart_quietly() {
                continue;
            }
            last_restart = Some(std::time::Instant::now());
            unresponsive_since = None;
            tracing::warn!("OpenRGB stopped answering; restarting it");
            match state.openrgb.restart(false).await {
                Ok(_) => tracing::info!("OpenRGB restarted"),
                Err(e) => tracing::warn!(%e, "OpenRGB restart failed"),
            }
            let _ = handle.emit("openrgb_status", state.openrgb.status());
        } else if !reachable {
            unresponsive_since = None;
            let since = *gone_since.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() < Duration::from_secs(5) || recently(last_start, 30) || !state.openrgb.responsible() {
                continue;
            }
            last_start = Some(std::time::Instant::now());
            gone_since = None;
            tracing::warn!("OpenRGB went away; starting it again");
            match state.openrgb.ensure_running().await {
                Ok(_) => tracing::info!("OpenRGB is back"),
                Err(e) => tracing::warn!(%e, "OpenRGB could not be started again"),
            }
            let _ = handle.emit("openrgb_status", state.openrgb.status());
        }
    }
}

/// A power event, with a way to say "done" for the urgent ones.
type PowerMessage = (PowerEvent, Option<std::sync::mpsc::Sender<()>>);

/// Turn the lights off and on as the PC sleeps, wakes, locks, turns its screen
/// off or sits idle, as far as the user's settings ask for it.
async fn follow_power(handle: AppHandle, engine: EngineHandle, mut events: tokio::sync::mpsc::UnboundedReceiver<PowerMessage>) {
    let mut idle_check = tokio::time::interval(Duration::from_secs(5));
    idle_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            message = events.recv() => {
                let Some((event, done)) = message else { return };
                let blackout = handle.state::<AppState>().lights.update(|p| p.apply(event));
                engine.send(Command::SetBlackout(blackout)).await;
                if event.is_urgent() {
                    // Give the engine a frame or two to send black before the OS moves on.
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                if let Some(done) = done {
                    let _ = done.send(());
                }
                if event == PowerEvent::Resume {
                    // USB devices often wake up in their own lighting mode: reload
                    // them so OpenRGB's direct mode is set again.
                    let engine = engine.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(4)).await;
                        engine.send(Command::RefreshDevices).await;
                    });
                }
            }
            _ = idle_check.tick() => {
                let state = handle.state::<AppState>();
                if state.lights.get().settings.idle_minutes == 0 {
                    continue;
                }
                let Some(seconds) = power::idle_seconds() else { continue };
                let blackout = state.lights.update(|p| p.set_idle_seconds(seconds));
                engine.send(Command::SetBlackout(blackout)).await;
            }
        }
    }
}

/// Watch for vendor lighting apps every twenty seconds; tell the UI when the set changes.
async fn watch_conflicts(handle: AppHandle) {
    loop {
        let running = tokio::task::spawn_blocking(conflicts::running_processes).await.unwrap_or_default();
        let found = conflicts::detect(&running);
        let state = handle.state::<AppState>();
        let changed = {
            let mut known = state.conflicts.lock().unwrap();
            let changed = *known != found;
            *known = found;
            changed
        };
        if changed {
            let visible = visible_conflicts(&state);
            if !visible.is_empty() {
                tracing::info!(apps = ?visible.iter().map(|c| c.name).collect::<Vec<_>>(), "lighting apps running alongside Lumen");
            }
            let _ = handle.emit("conflicts", visible);
        }
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

/// Notify when a wireless device runs low (see [`alerts`]).
async fn battery_alerts(handle: AppHandle, engine: EngineHandle) {
    let mut rx = engine.watch_snapshot();
    let mut alerts = alerts::BatteryAlerts::default();
    while rx.changed().await.is_ok() {
        let snap: Arc<Snapshot> = rx.borrow().clone();
        let enabled = handle.state::<AppState>().settings.lock().unwrap().battery_alerts.unwrap_or(true);
        for device in &snap.devices {
            let Some(battery) = device.battery else { continue };
            let key = if device.location.is_empty() { &device.name } else { &device.location };
            if let Some(alert) = alerts.check(key, &device.display_name, battery) {
                tracing::info!(device = %device.display_name, percent = battery.percent, notified = enabled, "battery low");
                if enabled {
                    if let Err(e) = handle.notification().builder().title(&alert.title).body(&alert.body).show() {
                        tracing::warn!(%e, "could not show the battery notification");
                    }
                }
            }
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(shortcuts::handle).build())
        .setup(|app| {
            let handle = app.handle().clone();
            let log_dir = diagnostics::init_logging(handle.path().app_log_dir().ok());
            tracing::info!(version = %handle.package_info().version, log_dir = ?log_dir, "Lumen starting");

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
                settings: Mutex::new(settings.clone()),
                data_dir: dir.clone(),
                lights: Lights::new(settings.power, settings.lights_off),
                conflicts: Mutex::new(Vec::new()),
                current_profile: Mutex::new(None),
                log_dir,
            });
            app.manage(updates::UpdateState::default());

            // Lights stay off if that is how the user left them.
            if settings.lights_off {
                let engine = engine.clone();
                tauri::async_runtime::spawn(async move { engine.send(Command::SetBlackout(Some(Blackout::User))).await });
            }

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

            // Power and session events arrive on the watcher's own thread; urgent
            // ones (sleep, shutdown) wait there until the lights are out.
            {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<PowerMessage>();
                power::watch(move |event| {
                    if event.is_urgent() {
                        let (done_tx, done_rx) = std::sync::mpsc::channel();
                        if tx.send((event, Some(done_tx))).is_ok() {
                            let _ = done_rx.recv_timeout(Duration::from_millis(1500));
                        }
                    } else {
                        let _ = tx.send((event, None));
                    }
                });
                tauri::async_runtime::spawn(follow_power(handle.clone(), engine.clone(), rx));
            }

            if settings.shortcuts {
                if let Err(e) = shortcuts::apply(&handle, true) {
                    tracing::warn!(%e, "keyboard shortcuts unavailable");
                }
            }

            tauri::async_runtime::spawn(forward_snapshots(handle.clone(), engine.clone()));
            tauri::async_runtime::spawn(forward_frames(handle.clone(), engine.clone()));
            tauri::async_runtime::spawn(autosave_scene(engine.clone(), dir));
            tauri::async_runtime::spawn(sidecar::run(engine.clone()));
            tauri::async_runtime::spawn(battery_alerts(handle.clone(), engine.clone()));
            tauri::async_runtime::spawn(watch_conflicts(handle.clone()));
            {
                let enabled = {
                    let h = handle.clone();
                    move || h.state::<AppState>().settings.lock().unwrap().auto_update.unwrap_or(true)
                };
                tauri::async_runtime::spawn(updates::auto_check(handle.clone(), enabled));
            }
            tauri::async_runtime::spawn(supervise_openrgb(handle.clone(), engine.clone()));

            tray::setup(&handle)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_layers,
            set_layout,
            set_brightness,
            set_scene,
            set_paused,
            set_lights_off,
            set_fps,
            refresh_devices,
            save_to_device,
            list_profiles,
            save_profile,
            load_profile,
            delete_profile,
            openrgb_status,
            start_openrgb,
            restart_openrgb,
            adopt_openrgb,
            release_openrgb,
            app_settings,
            set_auto_update,
            set_power_settings,
            set_battery_alerts,
            set_shortcuts,
            get_conflicts,
            dismiss_conflict,
            diagnostics,
            open_log_folder,
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
