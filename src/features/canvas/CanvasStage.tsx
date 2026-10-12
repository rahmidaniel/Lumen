// The virtual desk. Everything is drawn on one 2D canvas from the geometry the
// engine publishes: device bodies, keycaps / edge strips / dots lit with the
// latest frame, selection and hover outlines. Drag a device to move it, drag the
// background to pan, wheel to zoom.

import { Maximize2, Minus, Plus } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { IconButton } from "../../components/ui";
import { frames } from "../../lib/frames";
import type { DeviceGeometry, DeviceInfo, FramePayload, LedGlyph, Placement } from "../../lib/types";
import { useStore } from "../../store";
import { LightsOffNotice, NoDevicesHelp } from "./CanvasNotices";

const PAD = 56;
const NO_PLACEMENTS: Record<string, Placement> = {};
const NO_GEOMETRY: DeviceGeometry[] = [];
const NO_DEVICES: DeviceInfo[] = [];

interface Transform {
  s: number;
  tx: number;
  ty: number;
}

type Rgb = readonly [number, number, number];
const KEY_BASE: Rgb = [27, 32, 48];
const UNLIT = "rgb(33 37 50)";

// ---- geometry helpers -------------------------------------------------------

function corners(p: Placement): [number, number][] {
  const cx = p.x + p.width / 2;
  const cy = p.y + p.height / 2;
  const a = (p.rotation * Math.PI) / 180;
  const pts: [number, number][] = [
    [p.x, p.y],
    [p.x + p.width, p.y],
    [p.x, p.y + p.height],
    [p.x + p.width, p.y + p.height],
  ];
  return pts.map(([x, y]) => {
    const dx = x - cx;
    const dy = y - cy;
    return [cx + dx * Math.cos(a) - dy * Math.sin(a), cy + dx * Math.sin(a) + dy * Math.cos(a)];
  });
}

function fit(placements: Placement[], width: number, height: number): Transform {
  if (!placements.length || width <= 0 || height <= 0) return { s: 1, tx: 0, ty: 0 };
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const p of placements) {
    for (const [x, y] of corners(p)) {
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }
  const bw = Math.max(1, maxX - minX);
  const bh = Math.max(1, maxY - minY);
  const s = Math.min((width - PAD * 2) / bw, (height - PAD * 2) / bh);
  return { s, tx: (width - bw * s) / 2 - minX * s, ty: (height - bh * s) / 2 - minY * s };
}

/** World point → the device's local footprint coordinates. */
function toLocal(p: Placement, wx: number, wy: number): [number, number] {
  const cx = p.x + p.width / 2;
  const cy = p.y + p.height / 2;
  const a = (-p.rotation * Math.PI) / 180;
  const dx = wx - cx;
  const dy = wy - cy;
  return [dx * Math.cos(a) - dy * Math.sin(a) + p.width / 2, dx * Math.sin(a) + dy * Math.cos(a) + p.height / 2];
}

function toWorld(p: Placement, lx: number, ly: number): [number, number] {
  const cx = p.x + p.width / 2;
  const cy = p.y + p.height / 2;
  const a = (p.rotation * Math.PI) / 180;
  const dx = lx - p.width / 2;
  const dy = ly - p.height / 2;
  return [cx + dx * Math.cos(a) - dy * Math.sin(a), cy + dx * Math.sin(a) + dy * Math.cos(a)];
}

function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize({ width, height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size] as const;
}

// ---- drawing ---------------------------------------------------------------

function roundRectPath(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

function mousePath(ctx: CanvasRenderingContext2D, w: number, h: number) {
  ctx.beginPath();
  ctx.moveTo(w * 0.5, 0);
  ctx.bezierCurveTo(w * 0.96, 0, w, h * 0.22, w, h * 0.46);
  ctx.bezierCurveTo(w, h * 0.78, w * 0.84, h, w * 0.5, h);
  ctx.bezierCurveTo(w * 0.16, h, 0, h * 0.78, 0, h * 0.46);
  ctx.bezierCurveTo(0, h * 0.22, w * 0.04, 0, w * 0.5, 0);
  ctx.closePath();
}

type BodyState = "idle" | "hover" | "selected";

/** Graphics card from the fan side: I/O bracket on the left, fans across the shroud. */
function drawGpuBody(ctx: CanvasRenderingContext2D, w: number, h: number, radius: number, bracket: number, fans: number, state: BodyState, s: number) {
  // I/O bracket: a plate slightly taller than the card, with its screw slots.
  const ext = Math.min(8, h * 0.07);
  roundRectPath(ctx, 0, -ext, bracket, h + 2 * ext, Math.min(2, bracket * 0.2));
  ctx.fillStyle = "rgb(255 255 255 / 0.06)";
  ctx.fill();
  ctx.lineWidth = 1 / s;
  ctx.strokeStyle = "rgb(255 255 255 / 0.12)";
  ctx.stroke();
  for (const y of [-ext * 0.4, h + ext * 0.4]) {
    roundRectPath(ctx, bracket * 0.25, y - 0.6, bracket * 0.5, 1.2, 0.6);
    ctx.fillStyle = "rgb(0 0 0 / 0.4)";
    ctx.fill();
  }

  // Shroud.
  roundRectPath(ctx, bracket, 0, w - bracket, h, radius);
  ctx.fillStyle = "rgb(19 23 33 / 0.95)";
  ctx.fill();
  ctx.lineWidth = (state === "idle" ? 1 : 1.6) / s;
  ctx.strokeStyle = state === "selected" ? "#8b6cff" : state === "hover" ? "rgb(255 255 255 / 0.4)" : "rgb(255 255 255 / 0.13)";
  ctx.stroke();

  // Fans: ring, swept blades and hub. Decoration only.
  const pad = (w - bracket) * 0.03;
  const pitch = (w - bracket - 2 * pad) / fans;
  const r = Math.min(h * 0.4, pitch * 0.45);
  const hub = r * 0.22;
  const cy = h * 0.55;
  for (let i = 0; i < fans; i++) {
    const cx = bracket + pad + pitch * (i + 0.5);
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.fillStyle = "rgb(255 255 255 / 0.02)";
    ctx.fill();
    ctx.lineWidth = 1.2 / s;
    ctx.strokeStyle = "rgb(255 255 255 / 0.1)";
    ctx.stroke();
    ctx.lineWidth = 1 / s;
    ctx.strokeStyle = "rgb(255 255 255 / 0.045)";
    for (let k = 0; k < 9; k++) {
      const a = (k / 9) * Math.PI * 2;
      ctx.beginPath();
      ctx.moveTo(cx + hub * Math.cos(a), cy + hub * Math.sin(a));
      ctx.quadraticCurveTo(
        cx + r * 0.62 * Math.cos(a + 0.55),
        cy + r * 0.62 * Math.sin(a + 0.55),
        cx + r * 0.94 * Math.cos(a + 0.95),
        cy + r * 0.94 * Math.sin(a + 0.95),
      );
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.arc(cx, cy, hub, 0, Math.PI * 2);
    ctx.fillStyle = "rgb(255 255 255 / 0.07)";
    ctx.fill();
  }
}

function drawBody(ctx: CanvasRenderingContext2D, g: DeviceGeometry, state: BodyState, s: number) {
  const { width: w, height: h, body } = g;
  if (body.type === "gpu") {
    drawGpuBody(ctx, w, h, body.radius, body.bracket, body.fans, state, s);
    return;
  }
  if (body.type === "mouse") mousePath(ctx, w, h);
  else roundRectPath(ctx, 0, 0, w, h, body.radius);

  ctx.fillStyle =
    body.type === "keyboard" ? "rgb(19 23 33 / 0.95)" : body.type === "mouse" ? "rgb(23 27 39 / 0.95)" : "rgb(255 255 255 / 0.02)";
  ctx.fill();
  ctx.lineWidth = (state === "idle" ? 1 : 1.6) / s;
  ctx.strokeStyle = state === "selected" ? "#8b6cff" : state === "hover" ? "rgb(255 255 255 / 0.4)" : "rgb(255 255 255 / 0.13)";
  ctx.stroke();

  if (body.type === "mouse") {
    ctx.beginPath();
    ctx.moveTo(w * 0.5, 0);
    ctx.lineTo(w * 0.5, h * 0.3);
    ctx.lineWidth = 1 / s;
    ctx.strokeStyle = "rgb(255 255 255 / 0.1)";
    ctx.stroke();
  }
}

/** Trace a glyph's outline as the current path. */
function glyphPath(ctx: CanvasRenderingContext2D, led: LedGlyph, inflate = 1) {
  const g = led.glyph;
  switch (g.type) {
    case "key":
      roundRectPath(ctx, led.x - (g.w * inflate) / 2, led.y - (g.h * inflate) / 2, g.w * inflate, g.h * inflate, Math.min(g.w, g.h) * 0.2);
      break;
    case "cell":
      roundRectPath(ctx, led.x - (g.w * inflate) / 2, led.y - (g.h * inflate) / 2, g.w * inflate, g.h * inflate, Math.min(g.w, g.h) * 0.15);
      break;
    case "iso_enter": {
      // Upper half full width, lower half narrower and flush right.
      const w = g.w * inflate;
      const h = g.h * inflate;
      const x0 = led.x - w / 2;
      const y0 = led.y - h / 2;
      const notch = (g.w - g.lower_w) * inflate;
      const ym = y0 + h / 2;
      const r = Math.min(w, h) * 0.12;
      ctx.beginPath();
      ctx.moveTo(x0 + r, y0);
      ctx.arcTo(x0 + w, y0, x0 + w, y0 + h, r);
      ctx.arcTo(x0 + w, y0 + h, x0 + notch, y0 + h, r);
      ctx.arcTo(x0 + notch, y0 + h, x0 + notch, ym, r);
      ctx.arcTo(x0 + notch, ym, x0, ym, r);
      ctx.arcTo(x0, ym, x0, y0, r);
      ctx.arcTo(x0, y0, x0 + w, y0, r);
      ctx.closePath();
      break;
    }
    case "dot":
      ctx.beginPath();
      ctx.arc(led.x, led.y, g.r * inflate, 0, Math.PI * 2);
      break;
    case "strip":
      ctx.beginPath();
      g.points.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
      break;
  }
}

/** Crisp glyph, no blur — the glow is composited separately. */
function drawGlyph(ctx: CanvasRenderingContext2D, led: LedGlyph, c: Rgb | null, s: number) {
  const lit = c ? Math.max(c[0], c[1], c[2]) / 255 : 0;
  const css = c && lit > 0.02 ? `rgb(${c[0]} ${c[1]} ${c[2]})` : UNLIT;
  const g = led.glyph;
  glyphPath(ctx, led);
  if (g.type === "strip") {
    ctx.strokeStyle = css;
    ctx.lineWidth = g.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.stroke();
    return;
  }
  if (g.type === "key" || g.type === "iso_enter") {
    // Keycaps keep a dark base so unlit keys still read as keys.
    const k = 0.92 * lit;
    ctx.fillStyle = c ? `rgb(${Math.round(KEY_BASE[0] + (c[0] - KEY_BASE[0]) * k)} ${Math.round(KEY_BASE[1] + (c[1] - KEY_BASE[1]) * k)} ${Math.round(KEY_BASE[2] + (c[2] - KEY_BASE[2]) * k)})` : UNLIT;
    ctx.fill();
    ctx.lineWidth = 1 / s;
    ctx.strokeStyle = "rgb(255 255 255 / 0.07)";
    ctx.stroke();
    return;
  }
  ctx.fillStyle = css;
  ctx.fill();
}

/**
 * Cheap glow: translucent inflated copies of the glyph, composited additively.
 * No blur filter — a full-canvas Gaussian every frame was the single most
 * expensive thing the preview did.
 */
function drawGlowGlyph(ctx: CanvasRenderingContext2D, led: LedGlyph, c: Rgb) {
  const lit = Math.max(c[0], c[1], c[2]) / 255;
  if (lit < 0.08) return;
  const rgba = (a: number) => `rgb(${c[0]} ${c[1]} ${c[2]} / ${(a * lit).toFixed(3)})`;
  const g = led.glyph;
  if (g.type === "dot") {
    const r = g.r * 3.2;
    const grad = ctx.createRadialGradient(led.x, led.y, g.r * 0.4, led.x, led.y, r);
    grad.addColorStop(0, rgba(0.6));
    grad.addColorStop(1, rgba(0));
    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(led.x, led.y, r, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  // Keycaps get one tight halo so neighbours stay distinct; dots and strips
  // are small light sources and may bloom further.
  const passes: readonly (readonly [number, number])[] = g.type === "key" || g.type === "cell" || g.type === "iso_enter" ? [[1.22, 0.16]] : [[2.1, 0.09], [1.45, 0.17]];
  for (const [inflate, alpha] of passes) {
    if (g.type === "strip") {
      // Butt caps so neighbouring segments don't double up into bright beads;
      // extend the path a little into the gap between segments so it stays seamless.
      const pts = g.points;
      if (pts.length >= 2) {
        const ext = 2.5;
        const [ax, ay] = pts[0];
        const [bx, by] = pts[1];
        const [yx, yy] = pts[pts.length - 2];
        const [zx, zy] = pts[pts.length - 1];
        const d0 = Math.hypot(ax - bx, ay - by) || 1;
        const d1 = Math.hypot(zx - yx, zy - yy) || 1;
        ctx.beginPath();
        ctx.moveTo(ax + ((ax - bx) / d0) * ext, ay + ((ay - by) / d0) * ext);
        for (let i = 1; i < pts.length - 1; i++) ctx.lineTo(pts[i][0], pts[i][1]);
        ctx.lineTo(zx + ((zx - yx) / d1) * ext, zy + ((zy - yy) / d1) * ext);
      } else {
        glyphPath(ctx, led);
      }
      ctx.strokeStyle = rgba(alpha);
      ctx.lineWidth = g.width * inflate * 1.35;
      ctx.lineCap = "butt";
      ctx.lineJoin = "round";
      ctx.stroke();
    } else {
      glyphPath(ctx, led, inflate);
      ctx.fillStyle = rgba(alpha);
      ctx.fill();
    }
  }
}

/** Apply the world → device transform for one placement. */
function enterDevice(ctx: CanvasRenderingContext2D, t: Transform, p: Placement) {
  ctx.save();
  ctx.translate(t.tx, t.ty);
  ctx.scale(t.s, t.s);
  ctx.translate(p.x + p.width / 2, p.y + p.height / 2);
  ctx.rotate((p.rotation * Math.PI) / 180);
  ctx.translate(-p.width / 2, -p.height / 2);
}

interface SceneState {
  placements: Record<string, Placement>;
  geometry: DeviceGeometry[];
  devices: DeviceInfo[];
  selected: number | null;
}

function drawScene(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  st: SceneState,
  t: Transform,
  frame: FramePayload | null,
  hover: number | null,
) {
  ctx.clearRect(0, 0, width, height);
  const byDevice = new Map<number, number[]>();
  if (frame) for (const d of frame.devices) byDevice.set(d.id, d.rgb);
  const geometryById = new Map(st.geometry.map((g) => [g.device, g]));
  const colorOf = (rgb: number[] | undefined, led: LedGlyph): Rgb | null => {
    const i = led.led * 3;
    return rgb && i + 2 < rgb.length ? [rgb[i], rgb[i + 1], rgb[i + 2]] : null;
  };
  const placed = st.devices.flatMap((dev) => {
    const p = st.placements[String(dev.id)];
    const g = geometryById.get(dev.id);
    return p && g ? [{ dev, p, g, rgb: byDevice.get(dev.id) }] : [];
  });

  // Pass 1: bodies.
  for (const { dev, p, g } of placed) {
    const state: BodyState = dev.id === st.selected ? "selected" : dev.id === hover ? "hover" : "idle";
    enterDevice(ctx, t, p);
    drawBody(ctx, g, state, t.s);
    ctx.restore();
  }

  // Pass 2: glow, additive. Keycaps reach the edge of the case, so their glow
  // is clipped to it; edge-lit mats and mice are meant to spill light outward.
  ctx.save();
  ctx.globalCompositeOperation = "lighter";
  for (const { p, g, rgb } of placed) {
    enterDevice(ctx, t, p);
    if (g.body.type === "keyboard") {
      roundRectPath(ctx, 0, 0, g.width, g.height, g.body.radius);
      ctx.clip();
    }
    for (const led of g.leds) {
      const c = colorOf(rgb, led);
      if (c) drawGlowGlyph(ctx, led, c);
    }
    ctx.restore();
  }
  ctx.restore();

  // Pass 3: crisp glyphs and labels.
  const labels: { text: string; x: number; y: number; strong: boolean }[] = [];
  for (const { dev, p, g, rgb } of placed) {
    enterDevice(ctx, t, p);
    for (const led of g.leds) drawGlyph(ctx, led, colorOf(rgb, led), t.s);
    ctx.restore();

    const state: BodyState = dev.id === st.selected ? "selected" : dev.id === hover ? "hover" : "idle";
    if (state !== "idle") {
      const [wx, wy] = toWorld(p, p.width / 2, 0);
      labels.push({ text: dev.display_name, x: t.tx + wx * t.s, y: t.ty + wy * t.s - 12, strong: state === "selected" });
    }
  }

  ctx.font = "500 12px 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  for (const l of labels) {
    const w = ctx.measureText(l.text).width + 16;
    roundRectPath(ctx, l.x - w / 2, l.y - 11, w, 22, 11);
    ctx.fillStyle = l.strong ? "rgb(139 108 255 / 0.25)" : "rgb(16 19 26 / 0.9)";
    ctx.fill();
    ctx.fillStyle = l.strong ? "#e8ebf3" : "#b4bbcc";
    ctx.fillText(l.text, l.x, l.y + 0.5);
  }
}

// ---- component -------------------------------------------------------------

type Drag =
  | { kind: "device"; id: number; start: Placement; sx: number; sy: number }
  | { kind: "pan"; from: Transform; sx: number; sy: number }
  | { kind: "paint"; erase: boolean; done: Set<string> };

/** Distance from a point to a polyline. */
function distanceToPolyline(x: number, y: number, pts: [number, number][]): number {
  let best = Infinity;
  for (let i = 1; i < pts.length; i++) {
    const [ax, ay] = pts[i - 1];
    const [bx, by] = pts[i];
    const dx = bx - ax;
    const dy = by - ay;
    const len2 = dx * dx + dy * dy || 1;
    const t = Math.min(1, Math.max(0, ((x - ax) * dx + (y - ay) * dy) / len2));
    best = Math.min(best, Math.hypot(x - (ax + dx * t), y - (ay + dy * t)));
  }
  return best;
}

/** The LED glyph under a device-local point, if any. */
function ledAt(g: DeviceGeometry, lx: number, ly: number): LedGlyph | null {
  for (let i = g.leds.length - 1; i >= 0; i--) {
    const led = g.leds[i];
    const gl = led.glyph;
    switch (gl.type) {
      case "key":
      case "cell":
      case "iso_enter":
        if (Math.abs(lx - led.x) <= gl.w / 2 && Math.abs(ly - led.y) <= gl.h / 2) return led;
        break;
      case "dot":
        if (Math.hypot(lx - led.x, ly - led.y) <= Math.max(gl.r * 1.6, 4)) return led;
        break;
      case "strip":
        if (distanceToPolyline(lx, ly, gl.points) <= Math.max(gl.width, 4)) return led;
        break;
    }
  }
  return null;
}

export function CanvasStage() {
  const snapshot = useStore((s) => s.snapshot);
  const openrgb = useStore((s) => s.openrgb);
  const problem = useStore((s) => s.snapshot?.problem ?? null);
  const selected = useStore((s) => s.selectedDevice);
  const selectDevice = useStore((s) => s.selectDevice);
  const setPlacement = useStore((s) => s.setPlacement);
  const selectLayer = useStore((s) => s.selectLayer);
  const paintColor = useStore((s) => s.paintColor);
  const paintLeds = useStore((s) => s.paintLeds);
  // Painting: with a paint layer selected on the Effects tab, clicks colour LEDs instead of moving devices.
  const paintLayer = useStore((s) => (s.tab === "layers" ? (s.snapshot?.scene.layers.find((l) => l.id === s.selectedLayer && l.effect.type === "paint") ?? null) : null));
  const paintRef = useRef({ layer: paintLayer, color: paintColor });
  paintRef.current = { layer: paintLayer, color: paintColor };
  useEffect(() => {
    if (!paintLayer) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") selectLayer(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [paintLayer, selectLayer]);
  const [ref, size] = useSize<HTMLDivElement>();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const rawDevices = snapshot?.devices ?? NO_DEVICES;
  const placements = snapshot?.scene.layout.placements ?? NO_PLACEMENTS;
  const geometry = snapshot?.geometry ?? NO_GEOMETRY;
  // Z-order by footprint: big devices (the mat) at the back, small ones (the
  // mouse) on top, so drawing and picking both favour the smaller device.
  const devices = useMemo(() => {
    const area = (d: DeviceInfo) => {
      const p = placements[String(d.id)];
      return p ? p.width * p.height : 0;
    };
    return [...rawDevices].sort((a, b) => area(b) - area(a));
  }, [rawDevices, placements]);
  const fitT = useMemo(() => fit(Object.values(placements), size.width, size.height), [placements, size.width, size.height]);

  // Everything the draw loop needs, kept in refs so it never has to restart.
  const stateRef = useRef<SceneState>({ placements, geometry, devices, selected });
  stateRef.current = { placements, geometry, devices, selected };
  const fitRef = useRef(fitT);
  fitRef.current = fitT;
  const viewRef = useRef<Transform | null>(null); // null = follow the auto-fit
  const hoverRef = useRef<number | null>(null);
  // Redraw only when something changed: a new frame, interaction, or state.
  const dirtyRef = useRef(true);
  dirtyRef.current = true;
  const [zoomPct, setZoomPct] = useState(100);
  const current = () => viewRef.current ?? fitRef.current;

  useEffect(() => frames.subscribe(() => void (dirtyRef.current = true)), []);

  // Presenting a large canvas is what costs CPU in the WebView, not our draw
  // calls: cap the rate, and when the window is not focused drop to a few fps
  // at reduced resolution.
  const focusedRef = useRef(document.hasFocus());
  useEffect(() => {
    const onFocus = () => {
      focusedRef.current = true;
      dirtyRef.current = true;
    };
    const onBlur = () => {
      focusedRef.current = false;
      dirtyRef.current = true;
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    let raf = 0;
    let running = false;
    let lastDraw = 0;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      if (!dirtyRef.current) return;
      const focused = focusedRef.current;
      const now = performance.now();
      if (now - lastDraw < (focused ? 40 : 125)) return; // 25 fps focused, 8 fps otherwise
      lastDraw = now;
      dirtyRef.current = false;
      const scale = (window.devicePixelRatio || 1) * (focused ? 1 : 0.6);
      const w = Math.max(1, Math.floor(size.width * scale));
      const h = Math.max(1, Math.floor(size.height * scale));
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      ctx.setTransform(scale, 0, 0, scale, 0, 0);
      drawScene(ctx, size.width, size.height, stateRef.current, viewRef.current ?? fitRef.current, frames.latest, hoverRef.current);
    };
    const start = () => {
      if (running) return;
      running = true;
      dirtyRef.current = true;
      draw();
    };
    const stop = () => {
      running = false;
      cancelAnimationFrame(raf);
    };
    // No work at all while the window is hidden or minimised.
    const onVisibility = () => (document.hidden ? stop() : start());
    document.addEventListener("visibilitychange", onVisibility);
    if (!document.hidden) start();
    return () => {
      stop();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [size.width, size.height]);

  // Wheel zoom needs a non-passive listener to stop the page from scrolling.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      zoomAt(Math.exp(-e.deltaY * 0.0012), e.clientX - rect.left, e.clientY - rect.top);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ref.current]);

  const zoomAt = (factor: number, sx: number, sy: number) => {
    const t = current();
    const base = fitRef.current.s || 1;
    const s = Math.min(Math.max(t.s * factor, base * 0.2), base * 8);
    const k = s / t.s;
    viewRef.current = { s, tx: sx - (sx - t.tx) * k, ty: sy - (sy - t.ty) * k };
    dirtyRef.current = true;
    setZoomPct(Math.round((s / base) * 100));
  };
  const zoomCenter = (factor: number) => zoomAt(factor, size.width / 2, size.height / 2);
  const fitView = () => {
    viewRef.current = null;
    dirtyRef.current = true;
    setZoomPct(100);
  };

  const localPoint = (e: React.PointerEvent): [number, number] => {
    const rect = e.currentTarget.getBoundingClientRect();
    return [e.clientX - rect.left, e.clientY - rect.top];
  };
  const pick = (sx: number, sy: number): number | null => {
    const t = current();
    const wx = (sx - t.tx) / t.s;
    const wy = (sy - t.ty) / t.s;
    const { devices, placements } = stateRef.current;
    // Devices are ordered big → small; test the topmost (smallest) first.
    for (let i = devices.length - 1; i >= 0; i--) {
      const p = placements[String(devices[i].id)];
      if (!p) continue;
      const [lx, ly] = toLocal(p, wx, wy);
      if (lx >= 0 && lx <= p.width && ly >= 0 && ly <= p.height) return devices[i].id;
    }
    return null;
  };

  /** LED under a screen point: device, LED index, and a key for de-duplication. */
  const pickLed = (sx: number, sy: number): { device: number; led: number; key: string } | null => {
    const id = pick(sx, sy);
    if (id === null) return null;
    const t = current();
    const p = stateRef.current.placements[String(id)];
    const g = stateRef.current.geometry.find((x) => x.device === id);
    if (!p || !g) return null;
    const [lx, ly] = toLocal(p, (sx - t.tx) / t.s, (sy - t.ty) / t.s);
    const led = ledAt(g, lx, ly);
    return led ? { device: id, led: led.led, key: `${id}:${led.led}` } : null;
  };
  const paintAt = (sx: number, sy: number, d: Extract<Drag, { kind: "paint" }>) => {
    const hit = pickLed(sx, sy);
    const layer = paintRef.current.layer;
    if (!hit || !layer || d.done.has(hit.key)) return;
    d.done.add(hit.key);
    paintLeds(layer.id, hit.device, [hit.led], d.erase ? null : paintRef.current.color);
  };

  const drag = useRef<Drag | null>(null);
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 && e.button !== 1) return;
    const [sx, sy] = localPoint(e);
    e.currentTarget.setPointerCapture(e.pointerId);
    if (e.button === 0 && paintRef.current.layer) {
      const d: Drag = { kind: "paint", erase: e.shiftKey, done: new Set() };
      drag.current = d;
      paintAt(sx, sy, d);
      return;
    }
    const id = e.button === 0 ? pick(sx, sy) : null;
    if (id !== null) {
      drag.current = { kind: "device", id, start: stateRef.current.placements[String(id)], sx, sy };
      selectDevice(id);
    } else {
      drag.current = { kind: "pan", from: current(), sx, sy };
      if (e.button === 0) selectDevice(null);
    }
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const [sx, sy] = localPoint(e);
    const d = drag.current;
    if (!d) {
      if (paintRef.current.layer) {
        e.currentTarget.style.cursor = pickLed(sx, sy) ? "crosshair" : "default";
        return;
      }
      const id = pick(sx, sy);
      if (id !== hoverRef.current) {
        hoverRef.current = id;
        dirtyRef.current = true;
      }
      e.currentTarget.style.cursor = id !== null ? "grab" : "default";
      return;
    }
    if (d.kind === "paint") {
      paintAt(sx, sy, d);
      return;
    }
    dirtyRef.current = true;
    if (d.kind === "device") {
      const { s } = current();
      setPlacement(d.id, { ...d.start, x: d.start.x + (sx - d.sx) / s, y: d.start.y + (sy - d.sy) / s }, false);
      e.currentTarget.style.cursor = "grabbing";
    } else {
      viewRef.current = { s: d.from.s, tx: d.from.tx + (sx - d.sx), ty: d.from.ty + (sy - d.sy) };
      e.currentTarget.style.cursor = "grabbing";
    }
  };
  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    drag.current = null;
    e.currentTarget.style.cursor = "default";
    if (!d || d.kind !== "device") return;
    const [sx, sy] = localPoint(e);
    const { s } = current();
    setPlacement(d.id, { ...d.start, x: d.start.x + (sx - d.sx) / s, y: d.start.y + (sy - d.sy) / s }, true);
  };

  const empty = devices.length === 0;

  return (
    <div
      ref={ref}
      className="relative min-h-0 min-w-0 overflow-hidden dot-grid bg-bg touch-none"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    >
      <canvas ref={canvasRef} className="absolute inset-0" style={{ width: size.width, height: size.height }} />

      {empty && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2 text-center">
          {snapshot?.connected && !snapshot.detecting ? (
            <NoDevicesHelp />
          ) : (
            <>
              <div className="h-10 w-10 animate-spin rounded-full border-2 border-border-strong border-t-accent" />
              <p className="text-sm text-muted">
                {snapshot?.connected
                  ? "Looking for your devices…"
                  : (problem ?? (openrgb && !openrgb.exe ? "OpenRGB isn't installed. Install it, then press Start OpenRGB in Settings." : "Waiting for OpenRGB to report your devices…"))}
              </p>
            </>
          )}
        </div>
      )}

      {snapshot?.blackout && !empty && <LightsOffNotice reason={snapshot.blackout} />}

      {paintLayer && !empty && (
        <div className="pointer-events-none absolute left-3 top-3 flex items-center gap-2 rounded-lg border border-accent/40 bg-surface/90 px-3 py-1.5 text-xs backdrop-blur">
          <span className="h-2.5 w-2.5 rounded-full border border-white/30" style={{ background: `rgb(${Math.round(paintColor.r * 255)} ${Math.round(paintColor.g * 255)} ${Math.round(paintColor.b * 255)})` }} />
          <span className="text-text">Painting “{paintLayer.name}”</span>
          <span className="text-muted">click or drag across keys · Shift erases · Esc to stop</span>
        </div>
      )}

      {!empty && (
        <div
          className="absolute bottom-3 right-3 flex items-center gap-0.5 rounded-lg border border-border bg-surface/90 p-1 shadow-pop backdrop-blur"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <IconButton label="Zoom out" size="sm" onClick={() => zoomCenter(1 / 1.25)}>
            <Minus size={14} />
          </IconButton>
          <button className="h-7 min-w-12 rounded-md px-1.5 font-mono text-xs tabular-nums text-muted hover:bg-surface-3 hover:text-text" onClick={fitView} title="Fit to view">
            {zoomPct}%
          </button>
          <IconButton label="Zoom in" size="sm" onClick={() => zoomCenter(1.25)}>
            <Plus size={14} />
          </IconButton>
          <IconButton label="Fit to view" size="sm" onClick={fitView}>
            <Maximize2 size={14} />
          </IconButton>
        </div>
      )}
    </div>
  );
}
