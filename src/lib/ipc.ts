// Typed bridge to the Rust side. Falls back to the in-browser mock when the UI
// is not running inside Tauri, so the interface can be developed in a browser.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { FramePayload, Layer, Layout, OnboardEffect, OpenRgbStatus, Scene, Snapshot } from "./types";

export interface Backend {
  getSnapshot(): Promise<Snapshot>;
  setLayers(layers: Layer[]): Promise<void>;
  setLayout(layout: Layout): Promise<void>;
  setBrightness(brightness: number): Promise<void>;
  setScene(scene: Scene): Promise<void>;
  setPaused(paused: boolean): Promise<void>;
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
  onSnapshot(cb: (s: Snapshot) => void): () => void;
  onFrame(cb: (f: FramePayload) => void): () => void;
  onOpenrgbStatus(cb: (s: OpenRgbStatus) => void): () => void;
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
  setFps: (fps) => invoke("set_fps", { fps }),
  refreshDevices: () => invoke("refresh_devices"),
  saveToDevice: (deviceId, effect) => invoke("save_to_device", { deviceId, effect }),
  listProfiles: () => invoke<string[]>("list_profiles"),
  saveProfile: (name) => invoke<string[]>("save_profile", { name }),
  loadProfile: (name) => invoke("load_profile", { name }),
  deleteProfile: (name) => invoke<string[]>("delete_profile", { name }),
  openrgbStatus: () => invoke<OpenRgbStatus>("openrgb_status"),
  startOpenrgb: () => invoke<OpenRgbStatus>("start_openrgb"),
  onSnapshot: (cb) => subscribe<Snapshot>("snapshot", cb),
  onFrame: (cb) => subscribe<FramePayload>("frame", cb),
  onOpenrgbStatus: (cb) => subscribe<OpenRgbStatus>("openrgb_status", cb),
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
