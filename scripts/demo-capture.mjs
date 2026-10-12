#!/usr/bin/env node
// Records the animated demo at the top of the README (docs/screenshots/demo.webp)
// from the browser build of the UI and its mock backend: the four mock devices
// under the default rainbow wave, as one seamless loop.
//
//   npm run dev                       (in another terminal)
//   npm i --no-save sharp             (once; encodes the animation)
//   node scripts/demo-capture.mjs [--browser <path to Edge or Chrome>]
//
// Drives a headless Chromium over the DevTools protocol. The page clock is
// stepped one frame at a time and the mock's frame timer is fired by hand, so
// the result is smooth however slow the screenshots are, and frame N+1 would
// equal frame 1.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const URL_ = opt("url", "http://localhost:1420/");
const OUT = opt("out", join(dirname(fileURLToPath(import.meta.url)), "..", "docs", "screenshots", "demo.webp"));
const WIDTH = 1280;
const HEIGHT = 800;
// The mock's wave moves at 0.25 cycles/s, so 4 s is exactly one cycle.
const FPS = 25;
const LOOP_MS = 4000;
// Integer steps of at least 40 ms: the canvas draws at most every 40 ms of page time.
const STEP = Math.round(1000 / FPS);

const sharp = await import("sharp").then(
  (m) => m.default,
  () => {
    console.error("sharp is needed to encode the animation: npm i --no-save sharp");
    process.exit(1);
  },
);

function findBrowser() {
  const explicit = opt("browser");
  if (explicit) return explicit;
  const fixed = [
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/microsoft-edge",
  ];
  // Newer Edge installs keep the binary in a versioned EdgeCore folder.
  const core = "C:/Program Files (x86)/Microsoft/EdgeCore";
  if (existsSync(core)) for (const v of readdirSync(core)) fixed.push(`${core}/${v}/msedge.exe`);
  const found = fixed.find((p) => existsSync(p));
  if (!found) throw new Error("no Edge or Chrome found; pass --browser <path>");
  return found;
}

async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error(`cannot connect to ${wsUrl}`));
  });
  let next = 0;
  const pending = new Map();
  const waiters = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id !== undefined) {
      const p = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) p?.reject(new Error(`${p.method}: ${msg.error.message}`));
      else p?.resolve(msg.result);
    } else if (waiters.has(msg.method)) {
      waiters.get(msg.method)(msg.params);
      waiters.delete(msg.method);
    }
  };
  return {
    send: (method, params = {}) =>
      new Promise((resolve, reject) => {
        const id = ++next;
        pending.set(id, { resolve, reject, method });
        ws.send(JSON.stringify({ id, method, params }));
      }),
    once: (method) => new Promise((resolve) => waiters.set(method, resolve)),
    close: () => ws.close(),
  };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const profile = mkdtempSync(join(tmpdir(), "lumen-demo-"));
const browser = spawn(
  findBrowser(),
  [
    "--headless=new",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    `--window-size=${WIDTH},${HEIGHT}`,
    "--hide-scrollbars",
    "--mute-audio",
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ],
  { stdio: "ignore" },
);

try {
  // Chromium writes the port it picked to DevToolsActivePort in the profile.
  const portFile = join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100 && !existsSync(portFile); i++) await sleep(100);
  const port = readFileSync(portFile, "utf8").split("\n")[0].trim();
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = await connect(targets.find((t) => t.type === "page").webSocketDebuggerUrl);

  const evaluate = async (expression) => {
    const r = await page.send("Runtime.evaluate", { expression });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  };

  await page.send("Page.enable");
  await page.send("Emulation.setDeviceMetricsOverride", { width: WIDTH, height: HEIGHT, deviceScaleFactor: 1, mobile: false });
  // The canvas drops to a low rate and resolution while the window is unfocused.
  await page.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  // Take over the mock backend's 30 fps frame timer (src/lib/mock.ts) so that
  // each step renders exactly one frame and the canvas never draws a stale one.
  await page.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `(() => {
      const realSetInterval = window.setInterval.bind(window);
      window.__demo = { manual: false, tick: null, step: 0 };
      window.setInterval = (fn, ms, ...rest) => {
        if (typeof fn === "function" && Math.abs(ms - 1000 / 30) < 0.01) {
          window.__demo.tick = () => fn(...rest);
          return realSetInterval(() => window.__demo.manual || fn(...rest), ms);
        }
        return realSetInterval(fn, ms, ...rest);
      };
    })()`,
  });
  const loaded = page.once("Page.loadEventFired");
  await page.send("Page.navigate", { url: URL_ });
  await loaded;
  await sleep(2500); // fonts, layout and the panels' entrance animations

  // From here on the page clock only moves when we say so.
  await evaluate(`(() => {
      if (!window.__demo.tick) throw new Error("mock frame timer not found");
      const base = Math.ceil(performance.now()) + 1000;
      window.__demo.manual = true;
      performance.now = () => base + window.__demo.step;
      window.dispatchEvent(new Event("focus"));
    })()`);

  const frames = [];
  for (let t = 0; t < LOOP_MS; t += STEP) {
    await evaluate(`window.__demo.step = ${t}; window.__demo.tick()`);
    await sleep(100); // the canvas draws the new frame on the next animation frame
    const shot = await page.send("Page.captureScreenshot", { format: "png" });
    frames.push(Buffer.from(shot.data, "base64"));
    process.stdout.write(`\rframe ${frames.length}/${LOOP_MS / STEP}`);
  }
  process.stdout.write("\n");
  page.close();

  await sharp(frames, { join: { animated: true } })
    .webp({ quality: 80, effort: 6, loop: 0, delay: frames.map(() => STEP) })
    .toFile(OUT);
  console.log(`wrote ${OUT}`);
} finally {
  browser.kill();
  await sleep(500);
  rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}
