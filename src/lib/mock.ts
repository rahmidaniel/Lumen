// In-browser stand-in for the Rust backend, used when the UI runs outside Tauri
// (`npm run dev` in a browser). Fakes three devices and renders the layer stack
// with the same maths as crates/lumen-core so the preview looks right.

import type { Backend } from "./ipc";
import { hsv } from "./color";
import type {
  DeviceGeometry,
  DeviceInfo,
  Effect,
  FramePayload,
  Layer,
  Layout,
  LedGlyph,
  LedPoint,
  OpenRgbStatus,
  Palette,
  Placement,
  Rgb,
  Scene,
  Snapshot,
  Stop,
} from "./types";

// ---- fake hardware ---------------------------------------------------------

interface MockDevice extends DeviceInfo {
  /** Matrix of LED indices (null = empty cell) for keyboards. */
  matrix?: (number | null)[][];
  /** OpenRGB-style LED names. */
  names: string[];
}

// A tenkeyless layout on a 6×19 grid, using OpenRGB's key names.
const TKL_ROWS: (string | null)[][] = [
  ["Escape", null, "F1", "F2", "F3", "F4", null, "F5", "F6", "F7", "F8", null, "F9", "F10", "F11", "F12", "Print Screen", "Scroll Lock", "Pause/Break"],
  ["`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "Backspace", null, "Insert", "Home", "Page Up", null],
  ["Tab", "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\ (ANSI)", null, "Delete", "End", "Page Down", null],
  ["Caps Lock", "A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'", "Enter", null, null, null, null, null, null],
  ["Left Shift", "Z", "X", "C", "V", "B", "N", "M", ",", ".", "/", "Right Shift", null, null, null, null, "Up Arrow", null, null],
  ["Left Control", "Left Windows", "Left Alt", null, null, "Space", null, null, "Right Alt", "Right Fn", "Menu", "Right Control", null, null, null, "Left Arrow", "Down Arrow", "Right Arrow", null],
];

function buildKeyboard(): { matrix: (number | null)[][]; names: string[] } {
  const names: string[] = [];
  const matrix = TKL_ROWS.map((row) =>
    row.map((key) => {
      if (key === null) return null;
      names.push(`Key: ${key}`);
      return names.length - 1;
    }),
  );
  return { matrix, names };
}

const keyboard = buildKeyboard();

const devices: MockDevice[] = [
  {
    id: 0x1a2b3c01,
    name: "Razer DeathStalker V2 Pro TKL",
    display_name: "DeathStalker V2 Pro TKL",
    kind: 5,
    kind_label: "keyboard",
    vendor: "Razer",
    serial: "MOCK-KB",
    location: "mock",
    led_count: keyboard.names.length,
    zones: [{ name: "Keyboard", kind: 2, led_count: keyboard.names.length, matrix: [TKL_ROWS[0].length, TKL_ROWS.length] }],
    saveable_modes: [],
    streamable: true,
    battery: { percent: 64, charging: false },
    onboard: "razer",
    matrix: keyboard.matrix,
    names: keyboard.names,
  },
  {
    id: 0x1a2b3c02,
    name: "Razer Strider Chroma",
    display_name: "Strider Chroma",
    kind: 7,
    kind_label: "mousemat",
    vendor: "Razer",
    serial: "MOCK-MAT",
    location: "mock",
    led_count: 19,
    zones: [{ name: "Mat", kind: 1, led_count: 19, matrix: null }],
    saveable_modes: ["Spectrum Cycle", "Static", "Breathing"],
    streamable: true,
    battery: null,
    onboard: "razer",
    names: Array.from({ length: 19 }, (_, i) => `LED ${i}`),
  },
  {
    id: 0x1a2b3c03,
    name: "HyperX Pulsefire Haste 2 Wireless",
    display_name: "Pulsefire Haste 2",
    kind: 6,
    kind_label: "mouse",
    vendor: "HyperX",
    serial: "MOCK-MOUSE",
    location: "mock",
    led_count: 2,
    zones: [
      { name: "Scroll wheel", kind: 0, led_count: 1, matrix: null },
      { name: "Logo", kind: 0, led_count: 1, matrix: null },
    ],
    saveable_modes: [],
    streamable: true,
    battery: { percent: 98, charging: false, millivolts: 3897 },
    onboard: null,
    names: ["Scroll wheel", "Logo"],
  },
];

function defaultPlacement(kind: number): Placement {
  switch (kind) {
    case 5:
      return { x: 0, y: 0, width: 360, height: 140, rotation: 0 };
    case 6:
      return { x: 430, y: 8, width: 65, height: 125, rotation: 0 };
    case 7:
      return { x: -200, y: -120, width: 900, height: 400, rotation: 0 };
    default:
      return { x: 0, y: 480, width: 150, height: 100, rotation: 0 };
  }
}

// ---- geometry (mirrors crates/lumen-core/src/canvas.rs) --------------------

function keyUnits(name: string): number {
  switch (name) {
    case "Key: Space":
      return 6.25;
    case "Key: Right Shift":
      return 2.75;
    case "Key: Enter":
    case "Key: Left Shift":
      return 2.25;
    case "Key: Backspace":
    case "Key: Number Pad 0":
      return 2;
    case "Key: Caps Lock":
      return 1.75;
    case "Key: Tab":
    case "Key: \\ (ANSI)":
      return 1.5;
    case "Key: Left Control":
    case "Key: Left Windows":
    case "Key: Left Alt":
    case "Key: Right Alt":
    case "Key: Right Fn":
    case "Key: Menu":
    case "Key: Right Control":
    case "Key: Right Windows":
      return 1.25;
    default:
      return 1;
  }
}

function roundedLoop(w: number, h: number, radius: number, inset: number) {
  const r = Math.max(0.5, radius - inset);
  const [l, t, rt, b] = [inset, inset, w - inset, h - inset];
  type Piece = { len: number; at: (u: number) => [number, number] };
  const line = (x0: number, y0: number, x1: number, y1: number): Piece => ({
    len: Math.hypot(x1 - x0, y1 - y0),
    at: (u) => [x0 + (x1 - x0) * u, y0 + (y1 - y0) * u],
  });
  const arc = (cx: number, cy: number, a0: number, a1: number): Piece => ({
    len: r * Math.abs(a1 - a0),
    at: (u) => {
      const a = a0 + (a1 - a0) * u;
      return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
    },
  });
  const pieces = [
    line(l + r, t, rt - r, t),
    arc(rt - r, t + r, -Math.PI / 2, 0),
    line(rt, t + r, rt, b - r),
    arc(rt - r, b - r, 0, Math.PI / 2),
    line(rt - r, b, l + r, b),
    arc(l + r, b - r, Math.PI / 2, Math.PI),
    line(l, b - r, l, t + r),
    arc(l + r, t + r, Math.PI, Math.PI * 1.5),
  ];
  const length = pieces.reduce((a, p) => a + p.len, 0);
  return {
    length,
    pointAt(s: number): [number, number] {
      let d = ((s % length) + length) % length;
      for (const p of pieces) {
        if (d <= p.len) return p.at(p.len > 0 ? d / p.len : 0);
        d -= p.len;
      }
      return pieces[pieces.length - 1].at(1);
    },
  };
}

function deviceGeometry(d: MockDevice, p: Placement): DeviceGeometry {
  const w = Math.max(1, p.width);
  const h = Math.max(1, p.height);
  const leds: LedGlyph[] = [];

  if (d.matrix) {
    const rows = d.matrix.length;
    const cols = d.matrix[0].length;
    const cellW = w / cols;
    const cellH = h / rows;
    const padX = cellW * 0.07;
    const padY = cellH * 0.09;
    d.matrix.forEach((row, r) => {
      const lit = row.map((led, col) => (led === null ? null : { col, led })).filter((x): x is { col: number; led: number } => x !== null);
      lit.forEach(({ col, led }, i) => {
        const cx = (col + 0.5) * cellW;
        const cy = (r + 0.5) * cellH;
        const cap = keyUnits(d.names[led]) * cellW - 2 * padX;
        const leftMid = i > 0 ? ((lit[i - 1].col + 0.5) * cellW + cx) / 2 : null;
        const rightMid = i + 1 < lit.length ? ((lit[i + 1].col + 0.5) * cellW + cx) / 2 : null;
        const lo = (leftMid ?? 0) + padX;
        const hi = (rightMid ?? w) - padX;
        const kw = Math.max(Math.min(cap, hi - lo), cellW * 0.5);
        const wide = cap > cellW;
        let kx: number;
        if (wide && leftMid === null && rightMid !== null) kx = hi - kw / 2;
        else if (wide && leftMid !== null && rightMid === null) kx = lo + kw / 2;
        else kx = Math.min(Math.max(cx, lo + kw / 2), hi - kw / 2);
        leds.push({ zone: 0, led, x: kx, y: cy, glyph: { type: "key", w: kw, h: cellH - 2 * padY } });
      });
    });
    return { device: d.id, width: w, height: h, body: { type: "keyboard", radius: Math.min(8, Math.max(2, Math.min(w, h) * 0.04)) }, leds };
  }

  if (d.kind === 7) {
    const radius = Math.min(40, Math.max(4, Math.min(w, h) * 0.06));
    const width = Math.min(10, Math.max(3, Math.min(w, h) * 0.025));
    const path = roundedLoop(w, h, radius, width / 2 + 1);
    const n = d.led_count;
    const seg = path.length / n;
    const gap = Math.min(3, seg * 0.12);
    for (let i = 0; i < n; i++) {
      const s0 = i * seg + gap;
      const s1 = (i + 1) * seg - gap;
      const samples = Math.max(4, Math.ceil((s1 - s0) / 6));
      const points: [number, number][] = [];
      for (let k = 0; k <= samples; k++) points.push(path.pointAt(s0 + ((s1 - s0) * k) / samples));
      const [x, y] = path.pointAt((i + 0.5) * seg);
      leds.push({ zone: 0, led: i, x, y, glyph: { type: "strip", points, width } });
    }
    return { device: d.id, width: w, height: h, body: { type: "mat", radius }, leds };
  }

  // Mouse: single-LED zones stacked down the centre.
  const singles = d.zones.filter((z) => z.led_count === 1).length;
  let rank = 0;
  let offset = 0;
  for (const [zi, z] of d.zones.entries()) {
    if (z.led_count === 1) {
      const y = (h * (rank + 1)) / (singles + 1);
      rank++;
      leds.push({ zone: zi, led: offset, x: w / 2, y, glyph: { type: "dot", r: Math.min(8, Math.max(2, Math.min(w, h) * 0.09)) } });
    }
    offset += z.led_count;
  }
  return { device: d.id, width: w, height: h, body: { type: "mouse" }, leds };
}

function toWorld(p: Placement, lx: number, ly: number): [number, number] {
  const cx = p.x + p.width / 2;
  const cy = p.y + p.height / 2;
  const dx = lx - p.width / 2;
  const dy = ly - p.height / 2;
  const a = (p.rotation * Math.PI) / 180;
  return [cx + dx * Math.cos(a) - dy * Math.sin(a), cy + dx * Math.sin(a) + dy * Math.cos(a)];
}

function buildScene(layout: Layout): { geometry: DeviceGeometry[]; points: LedPoint[] } {
  const geometry: DeviceGeometry[] = [];
  const pts: LedPoint[] = [];
  devices.forEach((d, device_index) => {
    const p = layout.placements[String(d.id)];
    if (!p) return;
    const g = deviceGeometry(d, p);
    geometry.push(g);
    for (const led of g.leds) {
      const [x, y] = toWorld(p, led.x, led.y);
      pts.push({ device_index, device: d.id, zone: led.zone, led: led.led, x, y, u: 0, v: 0 });
    }
  });
  if (pts.length) {
    const xs = pts.map((q) => q.x);
    const ys = pts.map((q) => q.y);
    const [minX, maxX, minY, maxY] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    for (const q of pts) {
      q.u = maxX - minX > 1e-6 ? (q.x - minX) / (maxX - minX) : 0.5;
      q.v = maxY - minY > 1e-6 ? (q.y - minY) / (maxY - minY) : 0.5;
    }
  }
  return { geometry, points: pts };
}

// ---- effect maths (mirrors crates/lumen-core/src/effects.rs) --------------

function hash01(x: number): number {
  x ^= x >>> 16;
  x = Math.imul(x, 0x7feb352d);
  x ^= x >>> 15;
  x = Math.imul(x, 0x846ca68b);
  x ^= x >>> 16;
  return (x >>> 8) / (1 << 24);
}

const fract = (v: number) => v - Math.floor(v);
const lerp = (a: Rgb, b: Rgb, t: number): Rgb => ({ r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t });

function project(u: number, v: number, angle: number): number {
  const a = (angle * Math.PI) / 180;
  return (u - 0.5) * Math.cos(a) + (v - 0.5) * Math.sin(a) + 0.5;
}

function sampleStops(stops: Stop[], t: number, wrap: boolean): Rgb {
  if (stops.length === 0) return { r: 0, g: 0, b: 0 };
  if (stops.length === 1) return stops[0].color;
  const s = [...stops].sort((a, b) => a.pos - b.pos);
  const first = s[0];
  const last = s[s.length - 1];
  const wrapLerp = (tt: number) => lerp(last.color, first.color, Math.min(1, Math.max(0, (tt - last.pos) / Math.max(1e-6, first.pos + 1 - last.pos))));
  if (t <= first.pos) return wrap ? wrapLerp(t + 1) : first.color;
  if (t >= last.pos) return wrap ? wrapLerp(t) : last.color;
  for (let i = 0; i < s.length - 1; i++) {
    const a = s[i];
    const b = s[i + 1];
    if (t >= a.pos && t <= b.pos) return lerp(a.color, b.color, (t - a.pos) / Math.max(1e-6, b.pos - a.pos));
  }
  return last.color;
}

function samplePalette(p: Palette, t: number): Rgb {
  return p.type === "rainbow" ? hsv(t, 1, 1) : sampleStops(p.stops, fract(t), true);
}

function sampleEffect(e: Effect, u: number, v: number, t: number, seed: number): [Rgb, number] {
  switch (e.type) {
    case "static":
      return [e.color, 1];
    case "gradient":
      return [sampleStops(e.stops, Math.min(1, Math.max(0, project(u, v, e.angle))), false), 1];
    case "spectrum":
      return [hsv(fract(t * e.speed), 1, 1), 1];
    case "wave": {
      const phase = fract(project(u, v, e.angle) / Math.max(1e-3, e.wavelength) - t * e.speed);
      return [samplePalette(e.palette, phase), 1];
    }
    case "breathing": {
      const k = 0.5 - 0.5 * Math.cos(2 * Math.PI * t * e.speed);
      return [{ r: e.color.r * k, g: e.color.g * k, b: e.color.b * k }, 1];
    }
    case "starlight": {
      const cycleLen = 1 / Math.max(1e-3, e.speed);
      const local = t + hash01(seed) * cycleLen;
      const cycle = Math.floor(local / cycleLen);
      const frac = local / cycleLen - cycle;
      const roll = hash01((seed ^ Math.imul(cycle, 0x9e3779b9)) >>> 0);
      return roll < e.density ? [e.color, 1 - Math.abs(2 * frac - 1)] : [e.color, 0];
    }
  }
}

function blend(dst: Rgb, src: Rgb, alpha: number, mode: Layer["blend"]): Rgb {
  const a = Math.min(1, Math.max(0, alpha));
  if (a <= 0) return dst;
  let m: Rgb;
  switch (mode) {
    case "add":
      m = { r: dst.r + src.r, g: dst.g + src.g, b: dst.b + src.b };
      break;
    case "screen":
      m = { r: 1 - (1 - dst.r) * (1 - src.r), g: 1 - (1 - dst.g) * (1 - src.g), b: 1 - (1 - dst.b) * (1 - src.b) };
      break;
    case "multiply":
      m = { r: dst.r * src.r, g: dst.g * src.g, b: dst.b * src.b };
      break;
    default:
      m = src;
  }
  return lerp(dst, m, a);
}

function maskIncludes(layer: Layer, p: LedPoint): boolean {
  const m = layer.mask;
  if (m.type === "all") return true;
  if (m.type === "devices") return m.ids.includes(p.device);
  return m.zones.some((z) => z.device === p.device && z.zone === p.zone);
}

function renderFrame(scene: Scene, points: LedPoint[], t: number): FramePayload {
  const buffers = new Map<number, number[]>();
  for (const d of devices) buffers.set(d.id, new Array(d.led_count * 3).fill(0));
  const layers = scene.layers.filter((l) => l.enabled && l.opacity > 0);
  for (const p of points) {
    const seed = (Math.imul(p.device, 0x01000193) ^ Math.imul(p.led, 0x9e3779b9)) >>> 0;
    let c: Rgb = { r: 0, g: 0, b: 0 };
    for (const layer of layers) {
      if (!maskIncludes(layer, p)) continue;
      const [s, a] = sampleEffect(layer.effect, p.u, p.v, t, seed);
      c = blend(c, s, a * layer.opacity, layer.blend);
    }
    const buf = buffers.get(p.device)!;
    const k = scene.brightness;
    buf[p.led * 3] = Math.round(Math.min(1, Math.max(0, c.r * k)) * 255);
    buf[p.led * 3 + 1] = Math.round(Math.min(1, Math.max(0, c.g * k)) * 255);
    buf[p.led * 3 + 2] = Math.round(Math.min(1, Math.max(0, c.b * k)) * 255);
  }
  return { t, devices: devices.map((d) => ({ id: d.id, rgb: buffers.get(d.id)! })) };
}

// ---- backend ---------------------------------------------------------------

const defaultScene = (): Scene => ({
  layout: { placements: Object.fromEntries(devices.map((d) => [String(d.id), defaultPlacement(d.kind)])) },
  layers: [
    {
      id: "00000000-0000-4000-8000-000000000001",
      name: "Rainbow wave",
      enabled: true,
      effect: { type: "wave", angle: 0, speed: 0.25, wavelength: 1, palette: { type: "rainbow" } },
      opacity: 1,
      blend: "normal",
      mask: { type: "all" },
    },
  ],
  brightness: 1,
});

let scene = defaultScene();
let built = buildScene(scene.layout);
let paused = false;
let fps = 60;
const profiles: Record<string, Scene> = { "Desk default": defaultScene() };
const snapshotListeners = new Set<(s: Snapshot) => void>();
const frameListeners = new Set<(f: FramePayload) => void>();
const statusListeners = new Set<(s: OpenRgbStatus) => void>();
const started = performance.now();

const publicDevices: DeviceInfo[] = devices.map(({ matrix: _m, names: _n, ...info }) => info);
const snapshot = (): Snapshot => ({
  connected: true,
  paused,
  fps,
  devices: publicDevices,
  geometry: built.geometry,
  points: built.points,
  scene: structuredClone(scene),
});
const status = (): OpenRgbStatus => ({ exe: "C:\\Program Files\\OpenRGB\\OpenRGB.exe (mock)", server_reachable: true, started_by_lumen: true });
const publish = () => snapshotListeners.forEach((l) => l(snapshot()));

setInterval(() => {
  if (paused || frameListeners.size === 0) return;
  const f = renderFrame(scene, built.points, (performance.now() - started) / 1000);
  frameListeners.forEach((l) => l(f));
}, 1000 / 30);

export const mock: Backend = {
  async getSnapshot() {
    return snapshot();
  },
  async setLayers(layers) {
    scene = { ...scene, layers: structuredClone(layers) };
    publish();
  },
  async setLayout(layout) {
    scene = { ...scene, layout: structuredClone(layout) };
    built = buildScene(scene.layout);
    publish();
  },
  async setBrightness(brightness) {
    scene = { ...scene, brightness: Math.min(1, Math.max(0, brightness)) };
    publish();
  },
  async setScene(s) {
    scene = structuredClone(s);
    for (const d of devices) scene.layout.placements[String(d.id)] ??= defaultPlacement(d.kind);
    built = buildScene(scene.layout);
    publish();
  },
  async setPaused(p) {
    paused = p;
    publish();
  },
  async setFps(f) {
    fps = Math.min(120, Math.max(15, Math.round(f)));
    publish();
  },
  async refreshDevices() {
    publish();
  },
  async saveToDevice(deviceId, effect) {
    await new Promise((r) => setTimeout(r, 400));
    const d = devices.find((x) => x.id === deviceId);
    if (!d || d.onboard !== "razer") throw new Error("This device has no onboard memory Lumen can write.");
    console.info("[mock] stored on device", d.display_name, effect);
  },
  async listProfiles() {
    return Object.keys(profiles).sort();
  },
  async saveProfile(name) {
    profiles[name] = structuredClone(scene);
    return Object.keys(profiles).sort();
  },
  async loadProfile(name) {
    const s = profiles[name];
    if (!s) throw new Error(`profile not found: ${name}`);
    await mock.setScene(s);
  },
  async deleteProfile(name) {
    delete profiles[name];
    return Object.keys(profiles).sort();
  },
  async openrgbStatus() {
    return status();
  },
  async startOpenrgb() {
    return status();
  },
  onSnapshot(cb) {
    snapshotListeners.add(cb);
    return () => snapshotListeners.delete(cb);
  },
  onFrame(cb) {
    frameListeners.add(cb);
    return () => frameListeners.delete(cb);
  },
  onOpenrgbStatus(cb) {
    statusListeners.add(cb);
    return () => statusListeners.delete(cb);
  },
};
