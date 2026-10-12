#!/usr/bin/env node
// Rebuilds the updater manifest (latest.json) of a GitHub release from the
// signed assets attached to it, and uploads it.
//
//   node scripts/release-manifest.mjs v0.2.1 [--dry-run]
//
// tauri-action writes latest.json from every build job; when two jobs finish
// at the same moment one of them can fail with "Not Found - update-a-release-
// asset" and platform entries get lost. Re-run the failed jobs (`gh run rerun
// <id> --failed`), then run this to produce a manifest that lists every
// installer whose .sig is present. Needs the GitHub CLI, logged in.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const REPO = "rahmidaniel/lumen";
const [tag, ...flags] = process.argv.slice(2);
if (!tag) {
  console.error("usage: node scripts/release-manifest.mjs <tag> [--dry-run]");
  process.exit(1);
}
const dryRun = flags.includes("--dry-run");
const version = tag.replace(/^v/, "");
const gh = (...args) => execFileSync("gh", args, { stdio: ["ignore", "pipe", "inherit"] }).toString();

const dir = mkdtempSync(join(tmpdir(), "lumen-manifest-"));
gh("release", "download", tag, "-R", REPO, "-p", "*.sig", "-p", "latest.json", "-D", dir);
const files = readdirSync(dir);
const sigs = files.filter((f) => f.endsWith(".sig")).map((f) => f.slice(0, -4));

// Updater target keys per installer, as tauri-action names them.
const keysFor = (asset) => {
  if (/_x64-setup\.exe$/.test(asset)) return ["windows-x86_64", "windows-x86_64-nsis"];
  if (/_x64\.msi$/.test(asset)) return ["windows-x86_64-msi"];
  if (/_aarch64\.app\.tar\.gz$/.test(asset)) return ["darwin-aarch64", "darwin-aarch64-app"];
  if (/_x64\.app\.tar\.gz$/.test(asset)) return ["darwin-x86_64", "darwin-x86_64-app"];
  if (/_amd64\.AppImage$/.test(asset)) return ["linux-x86_64", "linux-x86_64-appimage"];
  if (/_amd64\.deb$/.test(asset)) return ["linux-x86_64-deb"];
  if (/x86_64\.rpm$/.test(asset)) return ["linux-x86_64-rpm"];
  return [];
};

const existing = files.includes("latest.json") ? JSON.parse(readFileSync(join(dir, "latest.json"), "utf8")) : {};
const manifest = {
  version,
  notes: existing.notes ?? `See the assets below.`,
  pub_date: existing.pub_date ?? new Date().toISOString(),
  platforms: {},
};
for (const asset of sigs) {
  const entry = { signature: readFileSync(join(dir, `${asset}.sig`), "utf8").trim(), url: `https://github.com/${REPO}/releases/download/${tag}/${asset}` };
  for (const key of keysFor(asset)) manifest.platforms[key] = entry;
}
const out = join(dir, "latest.json");
writeFileSync(out, JSON.stringify(manifest, null, 2) + "\n");
console.log(`${tag}: ${Object.keys(manifest.platforms).length} platform keys from ${sigs.length} signed assets`);
for (const k of Object.keys(manifest.platforms).sort()) console.log(`  ${k} -> ${manifest.platforms[k].url.split("/").pop()}`);
if (dryRun) {
  console.log(`dry run; manifest left at ${out}`);
} else {
  gh("release", "upload", tag, "-R", REPO, out, "--clobber");
  console.log("uploaded latest.json");
}
