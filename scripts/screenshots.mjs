#!/usr/bin/env node
// Captures the README screenshots (docs/screenshots/{devices,device-settings,
// effects,paint,settings}.png) from a running Lumen, through the DevTools
// protocol of its WebView2.
//
//   set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333
//   npm run tauri dev                       (or start an installed Lumen from that shell)
//   node scripts/screenshots.mjs [--port 9333] [--device "Razer Deathstalker"]
//
// The window should be at its default 1280×800. Lighting is switched on for the
// pictures if a "lights off" rule is active, and every setting the script
// touches is put back; the paint layer it adds is deleted again. Paint strokes
// are drags across the canvas at PAINT_ROWS (JSON `[[x0, x1, y], …]`), which
// match a tenkeyless keyboard in the middle of the desk; adjust for yours.

import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = opt("port", "9333");
const settingsDevice = opt("device", "Razer Deathstalker");
const rows = JSON.parse(process.env.PAINT_ROWS ?? "[[318,420,491],[324,410,506]]");
const outDir = join(dirname(fileURLToPath(import.meta.url)), "..", "docs", "screenshots");

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) throw new Error(`no Lumen window on port ${port}`);
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve) => (ws.onopen = resolve));
let next = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id) {
    pending.get(msg.id)?.(msg.result);
    pending.delete(msg.id);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = ++next;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const js = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result?.value;
};
const invoke = (cmd, payload = {}) => js(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)})`);
const rail = (label) => js(`[...document.querySelectorAll("nav button")].find(b => b.getAttribute("aria-label") === ${JSON.stringify(label)})?.click()`);
const clickButton = (text) => js(`[...document.querySelectorAll("aside button")].find(b => b.textContent.trim().startsWith(${JSON.stringify(text)}))?.click()`);
const deselect = () => js(`[...document.querySelectorAll("aside button")].find(b => b.className.includes("border-accent/60"))?.click()`);
const scrollPanel = (top) => js(`document.querySelector("aside").scrollTop = ${top}`);
const shot = async (name) => {
  const s = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(outDir, `${name}.png`), Buffer.from(s.data, "base64"));
  console.log(`saved docs/screenshots/${name}.png`);
};
const mouse = (type, x, y) => send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
const drag = async (x0, x1, y) => {
  await mouse("mouseMoved", x0, y);
  await mouse("mousePressed", x0, y);
  for (let x = x0; x <= x1; x += 4) {
    await mouse("mouseMoved", x, y);
    await sleep(8);
  }
  await mouse("mouseReleased", x1, y);
};

const { power, lights_off } = await invoke("app_settings");
try {
  await invoke("set_power_settings", { power: { ...power, display_off: false, lock: false, idle_minutes: 0 } });
  if (lights_off) await invoke("set_lights_off", { off: false });
  await sleep(2500);

  await rail("Devices");
  await sleep(600);
  await deselect();
  await sleep(500);
  await scrollPanel(0);
  await shot("devices");

  await clickButton(settingsDevice);
  await sleep(3000); // the settings are read from the device
  await scrollPanel(330);
  await sleep(400);
  await shot("device-settings");
  await deselect();

  await rail("Effects");
  await sleep(900);
  await shot("effects");

  await clickButton("Add layer");
  await sleep(500);
  await clickButton("Paint keys");
  await sleep(800);
  await js(`(() => {
    const input = document.querySelector("aside input[type=color]");
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, "#ffffff");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  })()`);
  await sleep(300);
  for (const [x0, x1, y] of rows) await drag(x0, x1, y);
  await sleep(1200);
  await shot("paint");
  await js(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }))`);
  await sleep(300);
  // The new layer is the topmost entry.
  await js(`document.querySelector('aside button[aria-label="Delete layer"]')?.click()`);
  await sleep(800);

  await rail("Settings");
  await sleep(900);
  await scrollPanel(0);
  await shot("settings");
  await rail("Devices");
} finally {
  await invoke("set_power_settings", { power });
  if (lights_off) await invoke("set_lights_off", { off: true });
  ws.close();
}
