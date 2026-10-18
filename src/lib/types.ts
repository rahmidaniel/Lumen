// Mirrors the serde output of the Rust types in crates/lumen-core and openrgb-sdk.

export interface ZoneInfo {
  name: string;
  kind: number;
  led_count: number;
  /** [width, height] for matrix zones. */
  matrix: [number, number] | null;
}

export interface Battery {
  /** 0–100. */
  percent: number;
  charging: boolean;
  /** Cell voltage when the device reports one. */
  millivolts?: number | null;
}

/** How a device can keep a power-on effect in its own memory. */
export type Onboard = "razer" | "openrgb";

export interface DeviceInfo {
  id: number;
  name: string;
  display_name: string;
  kind: number;
  kind_label: string;
  vendor: string;
  serial: string;
  location: string;
  led_count: number;
  zones: ZoneInfo[];
  saveable_modes: string[];
  streamable: boolean;
  battery: Battery | null;
  onboard: Onboard | null;
  /** Lumen can read and change this device's own settings. */
  configurable: boolean;
}

export interface ChoiceOption {
  value: number;
  label: string;
}

/** One adjustable device setting. */
export type DeviceSetting =
  | { type: "choice"; id: string; label: string; hint: string; value: number; options: ChoiceOption[] }
  | { type: "dpi_stages"; id: string; label: string; hint: string; stages: number[]; active: number; min: number; max: number; step: number; max_stages: number };

export interface DeviceSettings {
  device: number;
  settings: DeviceSetting[];
  /** "device": kept by the device; "lumen": Lumen puts them back after the device resets. */
  persistence: "device" | "lumen";
}

export type SettingChange = { type: "choice"; id: string; value: number } | { type: "dpi_stages"; stages: number[]; active: number };

/** Built-in effects a device can store as its power-on lighting. */
export type OnboardEffect =
  | { type: "off" }
  | { type: "static"; color: Rgb }
  | { type: "spectrum" }
  | { type: "wave"; direction: 1 | 2 }
  | { type: "breathing"; color: Rgb | null };

export interface LedPoint {
  device_index: number;
  device: number;
  zone: number;
  led: number;
  x: number;
  y: number;
  u: number;
  v: number;
}

export type Body =
  | { type: "rect"; radius: number }
  | { type: "keyboard"; radius: number; layout: "ansi" | "iso" | null }
  | { type: "mat"; radius: number }
  | { type: "mouse" }
  | { type: "gpu"; radius: number; bracket: number; fans: number };

export type Glyph =
  | { type: "key"; w: number; h: number }
  | { type: "iso_enter"; w: number; h: number; lower_w: number }
  | { type: "cell"; w: number; h: number }
  | { type: "dot"; r: number }
  | { type: "strip"; points: [number, number][]; width: number };

export interface LedGlyph {
  zone: number;
  led: number;
  x: number;
  y: number;
  glyph: Glyph;
}

/** Body outline and one glyph per LED, in the footprint's local millimetres. */
export interface DeviceGeometry {
  device: number;
  width: number;
  height: number;
  body: Body;
  leds: LedGlyph[];
}

export interface Placement {
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
}

/** Keyed by controller ID (JSON object keys are strings). */
export interface Layout {
  placements: Record<string, Placement>;
}

/** Channels are floats in 0..1. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface Stop {
  pos: number;
  color: Rgb;
}

export type Palette = { type: "rainbow" } | { type: "stops"; stops: Stop[] };

/** One colour on a set of LEDs of one device (LED indices in OpenRGB order). */
export interface PaintStroke {
  device: number;
  leds: number[];
  color: Rgb;
}

export type Effect =
  | { type: "static"; color: Rgb }
  | { type: "gradient"; stops: Stop[]; angle: number }
  | { type: "spectrum"; speed: number }
  | { type: "wave"; angle: number; speed: number; wavelength: number; palette: Palette }
  | { type: "breathing"; color: Rgb; speed: number }
  | { type: "starlight"; color: Rgb; density: number; speed: number }
  | { type: "paint"; strokes: PaintStroke[] };

export type EffectType = Effect["type"];

export type BlendMode = "normal" | "add" | "screen" | "multiply";

export type Mask =
  | { type: "all" }
  | { type: "devices"; ids: number[] }
  | { type: "zones"; zones: { device: number; zone: number }[] };

export interface Layer {
  id: string;
  name: string;
  enabled: boolean;
  effect: Effect;
  opacity: number;
  blend: BlendMode;
  mask: Mask;
}

export interface Scene {
  layout: Layout;
  layers: Layer[];
  brightness: number;
}

/** Why every LED is dark right now. */
export type Blackout = "user" | "sleep" | "display" | "lock" | "idle";

export interface Snapshot {
  connected: boolean;
  /** Why there is no connection, when the cause is more than "not running yet". */
  problem: string | null;
  paused: boolean;
  /** Set while the lights are off, with the reason. */
  blackout: Blackout | null;
  /** Connected, no devices yet, and OpenRGB may still be detecting them. */
  detecting: boolean;
  /** Render/stream rate. */
  fps: number;
  devices: DeviceInfo[];
  geometry: DeviceGeometry[];
  points: LedPoint[];
  scene: Scene;
}

export interface FramePayload {
  t: number;
  devices: { id: number; rgb: number[] }[];
}

export interface OpenRgbStatus {
  exe: string | null;
  bundled: boolean;
  server_reachable: boolean;
  started_by_lumen: boolean;
  /** How the running server relates to Lumen. */
  mode: "managed" | "child" | "service" | "external" | "none";
  /** The OpenRGB Windows service, when installed. */
  service: { running: boolean; disabled: boolean } | null;
  /** Lumen could take OpenRGB over on this machine. */
  can_manage: boolean;
  /** Lumen's service runs a different OpenRGB than this install's: update it. */
  stale: boolean;
  /** Lumen's service was set up by an older Lumen (OpenRGB at normal priority): update it. */
  outdated: boolean;
  /** Still managed through Lumen 0.2's scheduled task, which other programs can abuse. */
  legacy_task: boolean;
}

/** A running app that takes over lighting. */
export interface Conflict {
  id: string;
  name: string;
  /** Whose devices it takes over. */
  brands: string;
}

/** Which PC states turn the lights off. */
export interface PowerSettings {
  sleep: boolean;
  display_off: boolean;
  lock: boolean;
  /** 0 = never. */
  idle_minutes: number;
}

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
  date: string | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export interface AppSettings {
  version: string;
  auto_update: boolean;
  update: UpdateInfo | null;
  lights_off: boolean;
  power: PowerSettings;
  /** This platform reports sleep, display and lock changes. */
  power_supported: boolean;
  battery_alerts: boolean;
  shortcuts: boolean;
  log_dir: string | null;
}
