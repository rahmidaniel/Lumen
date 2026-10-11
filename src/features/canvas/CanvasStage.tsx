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

function drawBody(ctx: CanvasRenderingContext2D, g: DeviceGeometry, state: BodyState, s: number) {
  const { width: w, height: h, body } = g;
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

function drawGlyph(ctx: CanvasRenderingContext2D, led: LedGlyph, c: Rgb | null, s: number) {
  const lit = c ? Math.max(c[0], c[1], c[2]) / 255 : 0;
  const css = c && lit > 0.02 ? `rgb(${c[0]} ${c[1]} ${c[2]})` : UNLIT;
  if (c && lit > 0.08) {
    ctx.shadowColor = `rgb(${c[0]} ${c[1]} ${c[2]} / 0.9)`;
    ctx.shadowBlur = 8 + 16 * lit;
  } else {
    ctx.shadowBlur = 0;
  }
  const g = led.glyph;
  switch (g.type) {
    case "key": {
      // Keycaps keep a dark base so unlit keys still read as keys.
      const k = 0.92 * lit;
      ctx.fillStyle = c ? `rgb(${Math.round(KEY_BASE[0] + (c[0] - KEY_BASE[0]) * k)} ${Math.round(KEY_BASE[1] + (c[1] - KEY_BASE[1]) * k)} ${Math.round(KEY_BASE[2] + (c[2] - KEY_BASE[2]) * k)})` : UNLIT;
      roundRectPath(ctx, led.x - g.w / 2, led.y - g.h / 2, g.w, g.h, Math.min(g.w, g.h) * 0.2);
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.lineWidth = 1 / s;
      ctx.strokeStyle = "rgb(255 255 255 / 0.07)";
      ctx.stroke();
      break;
    }
    case "cell":
      ctx.fillStyle = css;
      roundRectPath(ctx, led.x - g.w / 2, led.y - g.h / 2, g.w, g.h, Math.min(g.w, g.h) * 0.15);
      ctx.fill();
      break;
    case "dot":
      ctx.fillStyle = css;
      ctx.beginPath();
      ctx.arc(led.x, led.y, g.r, 0, Math.PI * 2);
      ctx.fill();
      break;
    case "strip":
      ctx.strokeStyle = css;
      ctx.lineWidth = g.width;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      g.points.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
      ctx.stroke();
      break;
  }
  ctx.shadowBlur = 0;
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

  const labels: { text: string; x: number; y: number; strong: boolean }[] = [];
  for (const dev of st.devices) {
    const p = st.placements[String(dev.id)];
    const g = geometryById.get(dev.id);
    if (!p || !g) continue;
    const state: BodyState = dev.id === st.selected ? "selected" : dev.id === hover ? "hover" : "idle";

    ctx.save();
    ctx.translate(t.tx, t.ty);
    ctx.scale(t.s, t.s);
    ctx.translate(p.x + p.width / 2, p.y + p.height / 2);
    ctx.rotate((p.rotation * Math.PI) / 180);
    ctx.translate(-p.width / 2, -p.height / 2);
    drawBody(ctx, g, state, t.s);
    const rgb = byDevice.get(dev.id);
    for (const led of g.leds) {
      const i = led.led * 3;
      const c: Rgb | null = rgb && i + 2 < rgb.length ? [rgb[i], rgb[i + 1], rgb[i + 2]] : null;
      drawGlyph(ctx, led, c, t.s);
    }
    ctx.restore();

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
  | { kind: "pan"; from: Transform; sx: number; sy: number };

export function CanvasStage() {
  const snapshot = useStore((s) => s.snapshot);
  const openrgb = useStore((s) => s.openrgb);
  const selected = useStore((s) => s.selectedDevice);
  const selectDevice = useStore((s) => s.selectDevice);
  const setPlacement = useStore((s) => s.setPlacement);
  const [ref, size] = useSize<HTMLDivElement>();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const devices = snapshot?.devices ?? NO_DEVICES;
  const placements = snapshot?.scene.layout.placements ?? NO_PLACEMENTS;
  const geometry = snapshot?.geometry ?? NO_GEOMETRY;
  const fitT = useMemo(() => fit(Object.values(placements), size.width, size.height), [placements, size.width, size.height]);

  // Everything the draw loop needs, kept in refs so it never has to restart.
  const stateRef = useRef<SceneState>({ placements, geometry, devices, selected });
  stateRef.current = { placements, geometry, devices, selected };
  const fitRef = useRef(fitT);
  fitRef.current = fitT;
  const viewRef = useRef<Transform | null>(null); // null = follow the auto-fit
  const hoverRef = useRef<number | null>(null);
  const [zoomPct, setZoomPct] = useState(100);
  const current = () => viewRef.current ?? fitRef.current;

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    let raf = 0;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const dpr = window.devicePixelRatio || 1;
      const w = Math.floor(size.width * dpr);
      const h = Math.floor(size.height * dpr);
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawScene(ctx, size.width, size.height, stateRef.current, viewRef.current ?? fitRef.current, frames.latest, hoverRef.current);
    };
    draw();
    return () => cancelAnimationFrame(raf);
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
    setZoomPct(Math.round((s / base) * 100));
  };
  const zoomCenter = (factor: number) => zoomAt(factor, size.width / 2, size.height / 2);
  const fitView = () => {
    viewRef.current = null;
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
    for (let i = devices.length - 1; i >= 0; i--) {
      const p = placements[String(devices[i].id)];
      if (!p) continue;
      const [lx, ly] = toLocal(p, wx, wy);
      if (lx >= 0 && lx <= p.width && ly >= 0 && ly <= p.height) return devices[i].id;
    }
    return null;
  };

  const drag = useRef<Drag | null>(null);
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 && e.button !== 1) return;
    const [sx, sy] = localPoint(e);
    const id = e.button === 0 ? pick(sx, sy) : null;
    e.currentTarget.setPointerCapture(e.pointerId);
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
      const id = pick(sx, sy);
      hoverRef.current = id;
      e.currentTarget.style.cursor = id !== null ? "grab" : "default";
      return;
    }
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
          <div className="h-10 w-10 animate-spin rounded-full border-2 border-border-strong border-t-accent" />
          <p className="text-sm text-muted">
            {openrgb && !openrgb.exe ? "OpenRGB isn't installed. Install it, then press Start OpenRGB in Settings." : "Waiting for OpenRGB to report your devices…"}
          </p>
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
