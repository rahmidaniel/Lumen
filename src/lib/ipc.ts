// Typed bridge to the Rust side. Falls back to the in-browser mock when the UI
// is not running inside Tauri, so the interface can be developed in a browser.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppSettings, Conflict, FramePayload, Layer, Layout, OnboardEffect, OpenRgbStatus, PowerSettings, Scene, Snapshot, UpdateInfo, UpdateProgress } from "./types";

export interface Backend {
  getSnapshot(): Promise<Snapshot>;
  setLayers(layers: Layer[]): Promise<void>;
  setLayout(layout: Layout): Promise<void>;
  setBrightness(brightness: number): Promise<void>;
  setScene(scene: Scene): Promise<void>;
  setPaused(paused: boolean): Promise<void>;
  /** The lights switch; remembered across restarts. */
  setLightsOff(off: boolean): Promise<void>;
  setFps(fps: number): Promise<void>;
  refreshDevices(): Promise<void>;
  /** Store `effect` as the device's power-on lighting. */
  saveToDevice(deviceId: number, effect: OnboardEffect): Promise<void>;
  listProfiles(): Promise<string[]>;
  saveProfile(name: string): Promise<string[]>;
  loadProfile(name: string): Promise<void>;
  deleteProfile(name: string): Promise<string[]>;
  openrgbStatus(): Promise<OpenRgbStatus>;
  startOpenrgb(): Promise<OpenRgbStatus>;
  /** Restart OpenRGB: quietly when Lumen runs it, via an admin prompt for the Windows service. */
  restartOpenrgb(): Promise<OpenRgbStatus>;
  /** Windows: stop the OpenRGB service and run OpenRGB under Lumen's control (one admin prompt). */
  adoptOpenrgb(): Promise<OpenRgbStatus>;
  /** Windows: give OpenRGB back to its service (one admin prompt). */
  releaseOpenrgb(): Promise<OpenRgbStatus>;
  appSettings(): Promise<AppSettings>;
  setAutoUpdate(enabled: boolean): Promise<void>;
  setPowerSettings(power: PowerSettings): Promise<void>;
  setBatteryAlerts(enabled: boolean): Promise<void>;
  /** Fails when another app already owns one of the shortcuts. */
  setShortcuts(enabled: boolean): Promise<void>;
  getConflicts(): Promise<Conflict[]>;
  /** Stop warning about a vendor app; returns the remaining warnings. */
  dismissConflict(id: string): Promise<Conflict[]>;
  /** A Markdown report for bug reports. */
  diagnostics(): Promise<string>;
  openLogFolder(): Promise<void>;
  /** Ask the release feed; `null` means this is the latest version. */
  checkForUpdate(): Promise<UpdateInfo | null>;
  /** Download, verify and install the update found by the last check, then restart. */
  installUpdate(): Promise<void>;
  onSnapshot(cb: (s: Snapshot) => void): () => void;
  onFrame(cb: (f: FramePayload) => void): () => void;
  onOpenrgbStatus(cb: (s: OpenRgbStatus) => void): () => void;
  onUpdateAvailable(cb: (u: UpdateInfo) => void): () => void;
  onUpdateProgress(cb: (p: UpdateProgress) => void): () => void;
  onConflicts(cb: (c: Conflict[]) => void): () => void;
}

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function subscribe<T>(event: string, cb: (payload: T) => void): () => void {
  const unlisten = listen<T>(event, (e) => cb(e.payload));
  return () => {
    unlisten.then((un) => un());
  };
}

const tauriBackend: Backend = {
  getSnapshot: () => invoke<Snapshot>("get_snapshot"),
  setLayers: (layers) => invoke("set_layers", { layers }),
  setLayout: (layout) => invoke("set_layout", { layout }),
  setBrightness: (brightness) => invoke("set_brightness", { brightness }),
  setScene: (scene) => invoke("set_scene", { scene }),
  setPaused: (paused) => invoke("set_paused", { paused }),
  setLightsOff: (off) => invoke("set_lights_off", { off }),
  setFps: (fps) => invoke("set_fps", { fps }),
  refreshDevices: () => invoke("refresh_devices"),
  saveToDevice: (deviceId, effect) => invoke("save_to_device", { deviceId, effect }),
  listProfiles: () => invoke<string[]>("list_profiles"),
  saveProfile: (name) => invoke<string[]>("save_profile", { name }),
  loadProfile: (name) => invoke("load_profile", { name }),
  deleteProfile: (name) => invoke<string[]>("delete_profile", { name }),
  openrgbStatus: () => invoke<OpenRgbStatus>("openrgb_status"),
  startOpenrgb: () => invoke<OpenRgbStatus>("start_openrgb"),
  restartOpenrgb: () => invoke<OpenRgbStatus>("restart_openrgb"),
  adoptOpenrgb: () => invoke<OpenRgbStatus>("adopt_openrgb"),
  releaseOpenrgb: () => invoke<OpenRgbStatus>("release_openrgb"),
  appSettings: () => invoke<AppSettings>("app_settings"),
  setAutoUpdate: (enabled) => invoke("set_auto_update", { enabled }),
  setPowerSettings: (power) => invoke("set_power_settings", { power }),
  setBatteryAlerts: (enabled) => invoke("set_battery_alerts", { enabled }),
  setShortcuts: (enabled) => invoke("set_shortcuts", { enabled }),
  getConflicts: () => invoke<Conflict[]>("get_conflicts"),
  dismissConflict: (id) => invoke<Conflict[]>("dismiss_conflict", { id }),
  diagnostics: () => invoke<string>("diagnostics"),
  openLogFolder: () => invoke("open_log_folder"),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  installUpdate: () => invoke("install_update"),
  onSnapshot: (cb) => subscribe<Snapshot>("snapshot", cb),
  onFrame: (cb) => subscribe<FramePayload>("frame", cb),
  onOpenrgbStatus: (cb) => subscribe<OpenRgbStatus>("openrgb_status", cb),
  onUpdateAvailable: (cb) => subscribe<UpdateInfo>("update_available", cb),
  onUpdateProgress: (cb) => subscribe<UpdateProgress>("update_progress", cb),
  onConflicts: (cb) => subscribe<Conflict[]>("conflicts", cb),
};

let backendImpl: Backend | null = isTauri ? tauriBackend : null;

/** Resolve the backend; the mock is loaded lazily so Tauri builds never pull it in. */
export async function getBackend(): Promise<Backend> {
  if (!backendImpl) {
    const { mock } = await import("./mock");
    backendImpl = mock;
  }
  return backendImpl;
}
