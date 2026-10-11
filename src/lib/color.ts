import type { Rgb } from "./types";

export const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

export function rgbToHex(c: Rgb): string {
  return (
    "#" +
    [c.r, c.g, c.b]
      .map((v) =>
        Math.round(clamp01(v) * 255)
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
  );
}

export function hexToRgb(hex: string): Rgb {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return { r: 1, g: 1, b: 1 };
  const n = parseInt(m[1], 16);
  return { r: ((n >> 16) & 255) / 255, g: ((n >> 8) & 255) / 255, b: (n & 255) / 255 };
}

/** `h` in 0..1 (wraps), `s`/`v` in 0..1. Same formula as the Rust engine. */
export function hsv(h: number, s: number, v: number): Rgb {
  const h6 = (((h % 1) + 1) % 1) * 6;
  const i = Math.floor(h6);
  const f = h6 - i;
  const p = v * (1 - s);
  const q = v * (1 - s * f);
  const t = v * (1 - s * (1 - f));
  switch (i) {
    case 0:
      return { r: v, g: t, b: p };
    case 1:
      return { r: q, g: v, b: p };
    case 2:
      return { r: p, g: v, b: t };
    case 3:
      return { r: p, g: q, b: v };
    case 4:
      return { r: t, g: p, b: v };
    default:
      return { r: v, g: p, b: q };
  }
}

export const cssRgbBytes = (r: number, g: number, b: number) => `rgb(${r} ${g} ${b})`;

export const cssRgb = (c: Rgb) =>
  cssRgbBytes(Math.round(clamp01(c.r) * 255), Math.round(clamp01(c.g) * 255), Math.round(clamp01(c.b) * 255));
