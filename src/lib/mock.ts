// In-browser stand-in for the Rust backend, used when the UI runs outside Tauri
// (`npm run dev` in a browser). Fakes four devices and renders the layer stack
// with the same maths as crates/lumen-core so the preview looks right.

import type { Backend } from "./ipc";
import { hsv } from "./color";
import type {
  AppSettings,
  Blackout,
  Conflict,
  DeviceSetting,
  DeviceSettings,
  DeviceGeometry,
  DeviceInfo,
  Effect,
  FramePayload,
  Glyph,
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
import layoutTable from "./keyboard-layout.json";

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

/** `?layout=iso` in the browser shows the ISO variant: no ANSI backslash, `#` beside Enter, `\` beside Left Shift. */
function keyboardRows(): (string | null)[][] {
  const iso = typeof location !== "undefined" && new URLSearchParams(location.search).get("layout") === "iso";
  if (!iso) return TKL_ROWS;
  return TKL_ROWS.map((row, r) => {
    const out = row.map((k) => (k === "\\ (ANSI)" ? null : k));
    if (r === 3) out.splice(out.indexOf("Enter"), 0, "#");
    if (r === 4) out.splice(1, 0, "\\ (ISO)");
    while (out.length > row.length) out.pop();
    return out;
  });
}

function buildKeyboard(): { matrix: (number | null)[][]; names: string[] } {
  const names: string[] = [];
  const matrix = keyboardRows().map((row) =>
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
    configurable: true,
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
    configurable: true,
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
    configurable: true,
    names: ["Scroll wheel", "Logo"],
  },
  {
    id: 0x1a2b3c04,
    name: "ASUS TUF GeForce RTX 3080 Gaming OC",
    display_name: "ASUS TUF GeForce RTX 3080 Gaming OC",
    kind: 2,
    kind_label: "gpu",
    vendor: "ASUS",
    serial: "MOCK-GPU",
    location: "mock",
    led_count: 4,
    zones: [{ name: "GPU", kind: 1, led_count: 4, matrix: null }],
    saveable_modes: [],
    streamable: true,
    battery: null,
    onboard: null,
    configurable: false,
    names: ["LED 0", "LED 1", "LED 2", "LED 3"],
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
    case 2:
      return { x: 0, y: 330, width: 290, height: 110, rotation: 0 };
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

type SlotRow = [number, number, number, string];

/** Mirrors canvas.rs `physical_key_glyphs`: standard key names go to real positions, in key units. */
function physicalKeyboard(d: MockDevice, matrix: (number | null)[][], w: number, h: number): DeviceGeometry | null {
  const layout: "ansi" | "iso" = d.names.some((n) => n === "Key: \\ (ISO)" || n === "Key: #" || n === "Key: Enter (ISO)") ? "iso" : "ansi";
  const table = (layoutTable as unknown as Record<string, Record<string, SlotRow>>)[layout];
  const rowY = (row: number) => (row === 0 ? 0 : row + 0.5);
  type Item = { led: number; x: number; y: number; w: number; h: number; shape: string };
  const items: Item[] = [];
  const extras: { led: number; row: number; col: number }[] = [];
  const rows = matrix.length;
  const cols = matrix[0].length;
  matrix.forEach((row, r) =>
    row.forEach((led, c) => {
      if (led === null) return;
      const name = d.names[led] ?? "";
      if (!name) return;
      const s = table[name];
      if (s) items.push({ led, x: s[1], y: rowY(s[0]), w: s[2], h: s[3] === "plain" ? 1 : 2, shape: s[3] });
      else extras.push({ led, row: r, col: c });
    }),
  );
  if (!items.length) return null;
  let [minX, maxX, minY, maxY] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const it of items) {
    minX = Math.min(minX, it.x);
    maxX = Math.max(maxX, it.x + it.w);
    minY = Math.min(minY, it.y);
    maxY = Math.max(maxY, it.y + it.h);
  }
  for (const e of extras) {
    items.push({
      led: e.led,
      x: minX + ((e.col + 0.5) / cols) * (maxX - minX) - 0.5,
      y: rows === 6 ? rowY(e.row) : minY + (e.row / Math.max(1, rows - 1)) * (maxY - 1 - minY),
      w: 1,
      h: 1,
      shape: "plain",
    });
  }
  const margin = 0.4;
  const spanX = maxX - minX + 2 * margin;
  const spanY = maxY - minY + 2 * margin;
  const unit = Math.min(w / spanX, h / spanY);
  const ox = (w - spanX * unit) / 2 + (margin - minX) * unit;
  const oy = (h - spanY * unit) / 2 + (margin - minY) * unit;
  const gap = unit * 0.1;
  const leds: LedGlyph[] = items.map((it) => {
    const glyph: Glyph =
      it.shape === "iso_enter"
        ? { type: "iso_enter", w: it.w * unit - gap, h: it.h * unit - gap, lower_w: 1.25 * unit - gap }
        : { type: "key", w: it.w * unit - gap, h: it.h * unit - gap };
    return { zone: 0, led: it.led, x: ox + (it.x + it.w / 2) * unit, y: oy + (it.y + it.h / 2) * unit, glyph };
  });
  return { device: d.id, width: w, height: h, body: { type: "keyboard", radius: Math.min(8, Math.max(2, Math.min(w, h) * 0.04)), layout }, leds };
}

function deviceGeometry(d: MockDevice, p: Placement): DeviceGeometry {
  const w = Math.max(1, p.width);
  const h = Math.max(1, p.height);
  const leds: LedGlyph[] = [];

  if (d.matrix) {
    const physical = physicalKeyboard(d, d.matrix, w, h);
    if (physical) return physical;
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
    return { device: d.id, width: w, height: h, body: { type: "keyboard", radius: Math.min(8, Math.max(2, Math.min(w, h) * 0.04)), layout: null }, leds };
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

  if (d.kind === 2) {
    // Graphics card: segmented light bar along the top edge (second zone: bottom edge).
    const bracket = Math.min(14, Math.max(6, w * 0.045));
    const fans = Math.min(3, Math.max(1, Math.round((w - bracket) / (h * 0.95))));
    const width = Math.min(8, Math.max(3, h * 0.055));
    const inset = width / 2 + Math.min(6, Math.max(3, h * 0.05));
    let rank = 0;
    let offset = 0;
    for (const [zi, z] of d.zones.entries()) {
      const n = z.led_count;
      const y = rank === 0 ? inset : h - inset;
      const card = w - bracket;
      const x0 = bracket + card * (n === 1 ? 0.6 : 0.36);
      const x1 = w - card * 0.05;
      const seg = (x1 - x0) / n;
      const gap = Math.min(3, seg * 0.12);
      for (let i = 0; i < n; i++) {
        const a = x0 + seg * i + gap;
        const b = x0 + seg * (i + 1) - gap;
        leds.push({ zone: zi, led: offset + i, x: (a + b) / 2, y, glyph: { type: "strip", points: [[a, y], [b, y]], width } });
      }
      rank++;
      offset += n;
    }
    return { device: d.id, width: w, height: h, body: { type: "gpu", radius: Math.min(8, Math.max(2, h * 0.06)), bracket, fans }, leds };
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

function sampleEffect(e: Effect, u: number, v: number, t: number, seed: number, device: number, led: number): [Rgb, number] {
  switch (e.type) {
    case "static":
      return [e.color, 1];
    case "paint": {
      for (let i = e.strokes.length - 1; i >= 0; i--) {
        const s = e.strokes[i];
        if (s.device === device && s.leds.includes(led)) return [s.color, 1];
      }
      return [{ r: 0, g: 0, b: 0 }, 0];
    }
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
      const [s, a] = sampleEffect(layer.effect, p.u, p.v, t, seed, p.device, p.led);
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

// `?nodevices` shows the empty states, `?conflicts` a vendor-app warning,
// `?legacy` the warning about Lumen 0.2's scheduled task.
const query = new URLSearchParams(typeof location === "undefined" ? "" : location.search);
const noDevices = query.has("nodevices");

let scene = defaultScene();
let built = buildScene(scene.layout);
let paused = false;
let blackout: Blackout | null = null;
let fps = 60;
let app: AppSettings = {
  version: "0.3.0",
  auto_update: true,
  update: null,
  lights_off: false,
  power: { sleep: true, display_off: true, lock: false, idle_minutes: 0 },
  power_supported: true,
  battery_alerts: true,
  shortcuts: false,
  log_dir: "C:\\Users\\you\\AppData\\Local\\dev.lumen.studio\\logs (mock)",
};
let conflicts: Conflict[] = query.has("conflicts") ? [{ id: "razer-synapse", name: "Razer Synapse", brands: "Razer" }] : [];
const conflictListeners = new Set<(c: Conflict[]) => void>();

// Device settings for the mock keyboard and mouse, shaped like the real readings.
const deviceSettings = new Map<number, DeviceSetting[]>([
  [
    0x1a2b3c01,
    [
      { type: "choice", id: "polling_rate", label: "Polling rate", hint: "How often the device reports to the PC.", value: 1000, options: [1000, 500, 125].map((v) => ({ value: v, label: `${v} Hz` })) },
      { type: "choice", id: "idle_time", label: "Sleep after", hint: "Without input the device turns its radio off to save battery.", value: 900, options: [60, 120, 180, 300, 600, 900].map((v) => ({ value: v, label: `${v / 60} min` })) },
      { type: "choice", id: "low_battery", label: "Power saving below", hint: "Below this charge the device dims its lighting.", value: 9, options: [5, 9, 10, 15, 20, 25].map((v) => ({ value: v, label: `${v} %` })) },
    ],
  ],
  [
    0x1a2b3c03,
    [
      { type: "choice", id: "polling_rate", label: "Polling rate", hint: "How often the device reports to the PC.", value: 1000, options: [125, 250, 500, 1000].map((v) => ({ value: v, label: `${v} Hz` })) },
      { type: "dpi_stages", id: "dpi_stages", label: "DPI stages", hint: "The DPI button on the device steps through these.", stages: [400, 800, 1600, 3200], active: 1, min: 50, max: 26000, step: 50, max_stages: 5 },
    ],
  ],
]);
const settingsOf = (id: number): DeviceSettings => ({ device: id, settings: structuredClone(deviceSettings.get(id) ?? []), persistence: id === 0x1a2b3c03 ? "lumen" : "device" });
const profiles: Record<string, Scene> = { "Desk default": defaultScene() };
const snapshotListeners = new Set<(s: Snapshot) => void>();
const frameListeners = new Set<(f: FramePayload) => void>();
const statusListeners = new Set<(s: OpenRgbStatus) => void>();
const started = performance.now();

const publicDevices: DeviceInfo[] = devices.map(({ matrix: _m, names: _n, ...info }) => info);
const snapshot = (): Snapshot => ({
  connected: true,
  problem: null,
  paused,
  blackout,
  detecting: false,
  fps,
  devices: noDevices ? [] : publicDevices,
  geometry: noDevices ? [] : built.geometry,
  points: noDevices ? [] : built.points,
  scene: structuredClone(scene),
});
const status = (): OpenRgbStatus => ({
  exe: "C:\\Program Files\\OpenRGB\\OpenRGB.exe (mock)",
  bundled: true,
  server_reachable: true,
  started_by_lumen: true,
  mode: query.has("legacy") ? "managed" : "child",
  service: { running: false, disabled: true },
  can_manage: true,
  stale: false,
  legacy_task: query.has("legacy"),
});
const publish = () => snapshotListeners.forEach((l) => l(snapshot()));

setInterval(() => {
  if (paused || frameListeners.size === 0) return;
  const f = renderFrame(blackout ? { ...scene, brightness: 0 } : scene, built.points, (performance.now() - started) / 1000);
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
  async setLightsOff(off) {
    app = { ...app, lights_off: off };
    blackout = off ? "user" : null;
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
  async deviceSettings(deviceId) {
    await new Promise((r) => setTimeout(r, 300));
    return settingsOf(deviceId);
  },
  async setDeviceSetting(deviceId, change) {
    await new Promise((r) => setTimeout(r, 300));
    const list = deviceSettings.get(deviceId) ?? [];
    for (const s of list) {
      if (change.type === "choice" && s.type === "choice" && s.id === change.id) s.value = change.value;
      if (change.type === "dpi_stages" && s.type === "dpi_stages") {
        s.stages = [...change.stages];
        s.active = change.active;
      }
    }
    return settingsOf(deviceId);
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
  async restartOpenrgb() {
    await new Promise((r) => setTimeout(r, 400));
    return status();
  },
  async adoptOpenrgb() {
    throw new Error("Handing OpenRGB over needs the desktop app.");
  },
  async releaseOpenrgb() {
    throw new Error("Handing OpenRGB back needs the desktop app.");
  },
  async appSettings() {
    return structuredClone(app);
  },
  async setAutoUpdate(enabled) {
    app = { ...app, auto_update: enabled };
  },
  async setPowerSettings(power) {
    app = { ...app, power: { ...power } };
  },
  async setBatteryAlerts(enabled) {
    app = { ...app, battery_alerts: enabled };
  },
  async setShortcuts(enabled) {
    app = { ...app, shortcuts: enabled };
  },
  async getConflicts() {
    return conflicts;
  },
  async dismissConflict(id) {
    conflicts = conflicts.filter((c) => c.id !== id);
    conflictListeners.forEach((l) => l(conflicts));
    return conflicts;
  },
  async diagnostics() {
    const s = snapshot();
    return [
      "### Lumen diagnostics (browser mock)",
      "",
      `- **Lumen** ${app.version} · ${navigator.userAgent}`,
      `- **Engine**: connected yes, ${s.devices.length} devices, ${s.fps} fps`,
      "",
      ...s.devices.map((d) => `- ${d.display_name} (${d.vendor}, ${d.led_count} LEDs)`),
    ].join("\n");
  },
  async openLogFolder() {
    throw new Error("The browser mock writes no log file.");
  },
  onConflicts(cb) {
    conflictListeners.add(cb);
    return () => conflictListeners.delete(cb);
  },
  async checkForUpdate() {
    await new Promise((r) => setTimeout(r, 600));
    return null;
  },
  async installUpdate() {
    throw new Error("Updates are not available in the browser mock.");
  },
  onUpdateAvailable() {
    return () => {};
  },
  onUpdateProgress() {
    return () => {};
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
