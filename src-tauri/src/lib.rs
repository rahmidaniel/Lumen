//! Tauri shell: hosts the engine, exposes commands to the UI, forwards engine
//! state as events, keeps a tray icon, and remembers the current scene.

mod openrgb;
mod sidecar;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lumen_core::engine::{self, Command, EngineConfig, EngineHandle, FrameSet, Snapshot};
use lumen_core::{Layer, Layout, Profile, ProfileStore, Scene};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tracing_subscriber::EnvFilter;

struct AppState {
    engine: EngineHandle,
    profiles: ProfileStore,
    openrgb: openrgb::Manager,
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

async fn send(engine: &EngineHandle, cmd: Command) -> CmdResult {
    if engine.send(cmd).await { Ok(()) } else { Err("engine is not running".into()) }
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("Lumen"))
}

fn load_last_scene(dir: &PathBuf) -> Scene {
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

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Lumen", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Lumen", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app.default_window_icon().cloned().ok_or(tauri::Error::WindowNotFound)?;

    TrayIconBuilder::with_id("lumen-tray")
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let dir = data_dir(&handle);
            let scene = load_last_scene(&dir);

            // The engine spawns Tokio tasks, so create it on Tauri's runtime.
            let engine = tauri::async_runtime::block_on(async { engine::spawn(EngineConfig::default(), scene) });
            // Register state before any task can call `handle.state()`.
            app.manage(AppState {
                engine: engine.clone(),
                profiles: ProfileStore::new(dir.join("profiles")),
                openrgb: openrgb::Manager::new(),
            });

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

            // Forward engine state to the UI.
            {
                let handle = handle.clone();
                let mut rx = engine.watch_snapshot();
                tauri::async_runtime::spawn(async move {
                    while rx.changed().await.is_ok() {
                        let snap: Arc<Snapshot> = rx.borrow().clone();
                        let _ = handle.emit("snapshot", &*snap);
                    }
                });
            }
            {
                let handle = handle.clone();
                let mut frames = engine.subscribe_frames();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match frames.recv().await {
                            Ok(frame) => {
                                let _ = handle.emit("frame", FramePayload::from(&*frame));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
            tauri::async_runtime::spawn(autosave_scene(engine.clone(), dir));
            tauri::async_runtime::spawn(sidecar::run(engine.clone()));

            setup_tray(&handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides to tray; lighting keeps running.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_layers,
            set_layout,
            set_brightness,
            set_scene,
            set_paused,
            refresh_devices,
            save_to_device,
            list_profiles,
            save_profile,
            load_profile,
            delete_profile,
            openrgb_status,
            start_openrgb,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lumen");
}
