#!/usr/bin/env node
// Fetches the official OpenRGB 1.0 build for the platform being packaged into
// src-tauri/resources/openrgb, so Lumen's installer ships it ("complete package").
//
//   node scripts/fetch-openrgb.mjs [--force] [--platform windows|macos|linux]
//                                  [--arch x86_64|aarch64] [--dest DIR]
//
// Tauri runs it through `build.beforeBuildCommand` and exports TAURI_ENV_PLATFORM
// and TAURI_ENV_ARCH, so `tauri build --target …` selects the matching asset.
// Downloads are verified against pinned SHA-256 hashes (checked 2026-10-12) and
// cached in the OS temp directory.
//
// What ends up in the folder:
//   windows  OpenRGB.exe and its portable files (the official zip, unpacked)
//   macos    OpenRGB.zip exactly as published — Lumen unpacks OpenRGB.app on
//            first use, because the .app contains framework symlinks that would
//            not survive the bundler's resource copy
//   linux    OpenRGB.AppImage (executable) plus 60-openrgb.rules taken out of
//            the matching .deb; the deb/rpm packages install the rules under
//            /usr/lib/udev/rules.d so the bundled OpenRGB can reach devices
// plus LICENSE (GPL-2.0-or-later) and LUMEN_BUNDLE.txt (provenance).
//
// OpenRGB is GPL-2.0-or-later. Lumen talks to it over TCP as a separate program;
// bundling the binary is "mere aggregation", but the licence text and a pointer
// to the exact source must travel with it — see THIRD_PARTY_NOTICES.md.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "1.0";
const COMMIT = "81bbe18";
const RELEASE = `https://codeberg.org/OpenRGB/OpenRGB/releases/download/release_${VERSION}`;
const SOURCE = `https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_${VERSION}`;
const LICENSE_URL = `https://codeberg.org/OpenRGB/OpenRGB/raw/tag/release_${VERSION}/LICENSE`;

/** Official release assets, keyed by `<platform>-<arch>`. */
const ASSETS = {
  "windows-x86_64": {
    main: { file: `OpenRGB_${VERSION}_Windows_64_${COMMIT}.zip`, sha256: "182a52a3c97c4c4ae52c80286b4260c9666c51c3447dbc416ee8945f66192e90" },
  },
  "macos-aarch64": {
    main: { file: `OpenRGB_${VERSION}_MacOS_ARM64_${COMMIT}.zip`, sha256: "ee5e5da7cfa5d79b6604b81d791e3a7a7f2941025ca5de59d974d7a04d498be0" },
  },
  "macos-x86_64": {
    main: { file: `OpenRGB_${VERSION}_MacOS_Intel_${COMMIT}.zip`, sha256: "65e9c704eb0fd4ff941c08b183123fe7f9376ca82b5e2ae1f164abe55214fec8" },
  },
  "linux-x86_64": {
    main: { file: `OpenRGB_${VERSION}_x86_64_${COMMIT}.AppImage`, sha256: "a77d9fea9ab1e59e5ec2b5cec4ab22d50f0433594c4e83eb13de4e5c3cfadebf" },
    // Only its udev rules file is used: OpenRGB generates it at build time, so it is not in the repo.
    rules: { file: `openrgb_${VERSION}_amd64_bookworm_${COMMIT}.deb`, sha256: "be51c1d20fedb06f3c25baf61349c29c65ef92ab86f331b32a82d8c1eac217c3" },
  },
};
const RULES_MEMBER = "./usr/lib/udev/rules.d/60-openrgb.rules";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const opt = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};

function normalizePlatform(p) {
  switch ((p ?? "").toLowerCase()) {
    case "windows":
    case "win32":
      return "windows";
    case "macos":
    case "darwin":
      return "macos";
    case "linux":
      return "linux";
    default:
      return undefined;
  }
}

function normalizeArch(a) {
  switch ((a ?? "").toLowerCase()) {
    case "x86_64":
    case "x64":
    case "amd64":
      return "x86_64";
    case "aarch64":
    case "arm64":
      return "aarch64";
    default:
      return undefined;
  }
}

function fail(message) {
  console.error(`fetch-openrgb: ${message}`);
  process.exit(1);
}

function hashFile(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** Download `name` from the release (cached in the temp dir), verifying its SHA-256. */
async function download(name, sha256) {
  const cacheDir = join(tmpdir(), "lumen-openrgb");
  mkdirSync(cacheDir, { recursive: true });
  const cached = join(cacheDir, name);
  if (existsSync(cached) && hashFile(cached) === sha256) {
    console.log(`Using cached ${name}`);
    return cached;
  }
  const url = `${RELEASE}/${name}`;
  console.log(`Downloading ${url}`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  const bytes = Buffer.from(await res.arrayBuffer());
  const got = createHash("sha256").update(bytes).digest("hex");
  if (got !== sha256) throw new Error(`SHA-256 mismatch for ${name}: got ${got}, expected ${sha256}`);
  writeFileSync(cached, bytes);
  console.log(`SHA-256 ${got}`);
  return cached;
}

/** Run a command, returning its stdout as a Buffer; throws on failure. */
function capture(cmd, cmdArgs, input) {
  const r = spawnSync(cmd, cmdArgs, { input, stdio: ["pipe", "pipe", "inherit"], maxBuffer: 512 * 1024 * 1024 });
  if (r.error) throw new Error(`${cmd}: ${r.error.message}`);
  if (r.status !== 0) throw new Error(`${cmd} ${cmdArgs.join(" ")} exited with ${r.status}`);
  return r.stdout;
}

function available(cmd) {
  const r = spawnSync(process.platform === "win32" ? "where" : "which", [cmd], { stdio: "ignore" });
  return r.status === 0;
}

/** bsdtar: Windows ships it in System32 (GNU tar from Git may shadow it on PATH); macOS's tar is bsdtar. */
function systemTar() {
  if (process.platform === "win32") {
    const t = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
    if (existsSync(t)) return t;
  }
  return "tar";
}

function unzipWindows(zip, into) {
  capture(systemTar(), ["-xf", zip, "-C", into]);
  // The zip may wrap everything in one top-level folder; flatten it.
  if (!existsSync(join(into, "OpenRGB.exe"))) {
    const inner = readdirSync(into).find((e) => statSync(join(into, e)).isDirectory() && existsSync(join(into, e, "OpenRGB.exe")));
    if (inner) {
      for (const e of readdirSync(join(into, inner))) renameSync(join(into, inner, e), join(into, e));
      rmSync(join(into, inner), { recursive: true, force: true });
    }
  }
  if (!existsSync(join(into, "OpenRGB.exe"))) throw new Error("OpenRGB.exe not found after extraction");
}

/** Pull 60-openrgb.rules out of the .deb without installing anything. */
function extractRules(deb, out) {
  let data;
  if (available("dpkg-deb")) {
    const tarball = capture("dpkg-deb", ["--fsys-tarfile", deb]);
    data = capture("tar", ["-xOf", "-", RULES_MEMBER], tarball);
  } else {
    // bsdtar reads ar archives and xz-compressed members directly.
    const tar = systemTar();
    const members = capture(tar, ["-tf", deb]).toString().split(/\r?\n/);
    const dataMember = members.find((m) => m.startsWith("data.tar"));
    if (!dataMember) throw new Error(`no data.tar member in ${deb}`);
    const tarball = capture(tar, ["-xOf", deb, dataMember]);
    data = capture(tar, ["-xOf", "-", RULES_MEMBER], tarball);
  }
  if (data.length < 1000 || !data.toString("utf8", 0, 200).includes("udev")) throw new Error("udev rules extraction produced no rules");
  writeFileSync(out, data);
}

async function main() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const platform = normalizePlatform(opt("--platform") ?? process.env.TAURI_ENV_PLATFORM ?? process.platform);
  const arch = normalizeArch(opt("--arch") ?? process.env.TAURI_ENV_ARCH ?? process.arch);
  if (!platform || !arch) fail(`unsupported platform/arch (${process.env.TAURI_ENV_PLATFORM ?? process.platform} / ${process.env.TAURI_ENV_ARCH ?? process.arch})`);
  const key = `${platform}-${arch}`;
  const asset = ASSETS[key];
  if (!asset) fail(`no pinned OpenRGB ${VERSION} build for ${key} (have: ${Object.keys(ASSETS).join(", ")})`);

  const dest = resolve(opt("--dest") ?? join(root, "src-tauri", "resources", "openrgb"));
  const stamp = join(dest, "LUMEN_BUNDLE.txt");
  const markers = { windows: ["OpenRGB.exe"], macos: ["OpenRGB.zip"], linux: ["OpenRGB.AppImage", "60-openrgb.rules"] }[platform];
  if (!flag("--force") && markers.every((f) => existsSync(join(dest, f))) && existsSync(stamp) && readFileSync(stamp, "utf8").includes(asset.main.file)) {
    console.log(`OpenRGB ${VERSION} (${asset.main.file}) already present in ${dest} (use --force to refetch)`);
    return;
  }

  const main = await download(asset.main.file, asset.main.sha256);
  const rules = asset.rules ? await download(asset.rules.file, asset.rules.sha256) : undefined;

  rmSync(dest, { recursive: true, force: true });
  mkdirSync(dest, { recursive: true });
  // Keep the folder tracked even when empty: tauri-build checks that resource paths exist.
  writeFileSync(join(dest, ".gitkeep"), "");

  switch (platform) {
    case "windows":
      unzipWindows(main, dest);
      break;
    case "macos":
      copyFileSync(main, join(dest, "OpenRGB.zip"));
      break;
    case "linux": {
      const appimage = join(dest, "OpenRGB.AppImage");
      copyFileSync(main, appimage);
      chmodSync(appimage, 0o755);
      extractRules(rules, join(dest, "60-openrgb.rules"));
      break;
    }
  }

  // GPL-2.0 text must travel with the binary.
  const licenseCache = join(tmpdir(), "lumen-openrgb", "LICENSE");
  if (!existsSync(licenseCache)) {
    const res = await fetch(LICENSE_URL, { redirect: "follow" });
    if (!res.ok) throw new Error(`${LICENSE_URL}: HTTP ${res.status}`);
    writeFileSync(licenseCache, Buffer.from(await res.arrayBuffer()));
  }
  copyFileSync(licenseCache, join(dest, "LICENSE"));

  const lines = [
    `OpenRGB ${VERSION} (commit ${COMMIT}), bundled unmodified for ${key} from`,
    `  ${RELEASE}/${asset.main.file}`,
    `  SHA-256: ${asset.main.sha256}`,
  ];
  if (asset.rules) {
    lines.push(`60-openrgb.rules taken from`, `  ${RELEASE}/${asset.rules.file}`, `  SHA-256: ${asset.rules.sha256}`);
  }
  lines.push(`Licence: GPL-2.0-or-later — see LICENSE in this folder. Source: ${SOURCE}`, "");
  writeFileSync(stamp, lines.join("\n"));

  let size = 0;
  const walk = (dir) => {
    for (const e of readdirSync(dir)) {
      const p = join(dir, e);
      const s = statSync(p);
      if (s.isDirectory()) walk(p);
      else size += s.size;
    }
  };
  walk(dest);
  console.log(`OpenRGB ${VERSION} for ${key} ready in ${dest} (${(size / 1048576).toFixed(1)} MB)`);
}

main().catch((e) => fail(e.message ?? String(e)));
