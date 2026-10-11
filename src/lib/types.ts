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
}

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
  | { type: "keyboard"; radius: number }
  | { type: "mat"; radius: number }
  | { type: "mouse" };

export type Glyph =
  | { type: "key"; w: number; h: number }
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

export type Effect =
  | { type: "static"; color: Rgb }
  | { type: "gradient"; stops: Stop[]; angle: number }
  | { type: "spectrum"; speed: number }
  | { type: "wave"; angle: number; speed: number; wavelength: number; palette: Palette }
  | { type: "breathing"; color: Rgb; speed: number }
  | { type: "starlight"; color: Rgb; density: number; speed: number };

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

export interface Snapshot {
  connected: boolean;
  paused: boolean;
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
  server_reachable: boolean;
  started_by_lumen: boolean;
}
