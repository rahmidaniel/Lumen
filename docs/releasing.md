# Releasing

GitHub Actions does the cross-platform builds (`.github/workflows/release.yml`):

1. Bump the version in `src-tauri/tauri.conf.json`, `package.json` and the workspace
   `Cargo.toml`, add a section to `CHANGELOG.md`, commit.
2. Tag and push: `git tag vX.Y.Z && git push origin main vX.Y.Z`.
3. The workflow builds Windows (NSIS), macOS (Apple Silicon and Intel DMGs) and Linux
   (deb, rpm, AppImage) in parallel, signs them for the updater, and attaches them plus
   the `latest.json` update manifest to a **draft** release named after the tag. Paste
   the changelog section into the release notes, review it on GitHub and publish; the
   updater only sees published, non-pre-release releases.
4. If two jobs finish at the same moment, one can fail while updating the shared
   `latest.json` ("Not Found - update-a-release-asset") and the manifest may miss
   platforms. Re-run the failed jobs (`gh run rerun <run id> --failed`), then rebuild the
   manifest from the signed assets with `node scripts/release-manifest.mjs vX.Y.Z` before
   publishing.

The OpenRGB build assets come from the `openrgb-1.0` pre-release of this repository (a
byte-for-byte mirror of the official files, SHA-256-pinned in `scripts/fetch-openrgb.mjs`),
with Codeberg as the fallback; Codeberg has refused automated downloads before.

`ci.yml` runs `npm run build` and `cargo test --workspace` on all three OSes for every
push and pull request.

## Updater signing

Lumen checks GitHub Releases for newer versions: on demand from Settings → Updates, and
once a day in the background (switchable off there). Nothing is installed until the user
presses *Install and restart*. The release workflow signs every installer with the
project's updater key and attaches a `latest.json` manifest to the release; the app
verifies the signature against the public key in `src-tauri/tauri.conf.json` before
installing, then restarts.

The private key lives outside the repository: `~/.tauri/lumen.key` on the maintainer's
machine (a local `tauri build` reads it through `TAURI_SIGNING_PRIVATE_KEY`) and the
`TAURI_SIGNING_PRIVATE_KEY` secret in GitHub Actions. Losing it means existing installs can
no longer verify updates, so keep a backup. The feed URL
`https://github.com/rahmidaniel/lumen/releases/latest/download/latest.json` is only served
by GitHub for public repositories, so the repository has to stay public for updates to
reach users.

## Code signing

Not set up yet, which is why users see SmartScreen and Gatekeeper warnings. For macOS,
add the `APPLE_*` secrets that [tauri-action](https://github.com/tauri-apps/tauri-action)
documents and pass them through the workflow's `env:`; for Windows, a code-signing
certificate or a signing service (the SignPath Foundation signs open-source projects for
free) can be wired into `bundle.windows` in the Tauri config.
