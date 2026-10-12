# How Lumen runs OpenRGB

Lumen does device I/O through [OpenRGB](https://openrgb.org)'s SDK server (protocol v6
over TCP, `127.0.0.1:6742`). This page covers how the OpenRGB it talks to gets onto the
machine, how Lumen finds it, and how the two lifecycles are tied together.

## Bundling

The packages are self-contained: `scripts/fetch-openrgb.mjs` (run automatically by the
build) downloads the official OpenRGB 1.0 asset for the target platform and
architecture, verifies its SHA-256 and places it under `src-tauri/resources/openrgb/`,
which the bundler ships as `openrgb/` next to the app:

| platform | bundled form | how Lumen runs it |
|---|---|---|
| Windows | portable `OpenRGB.exe` | directly |
| macOS | the official `OpenRGB.zip` | unpacked once into Lumen's data dir with `ditto` (the `.app` holds framework symlinks the bundler would flatten) |
| Linux | `OpenRGB.AppImage` + `60-openrgb.rules` | AppImage with `APPIMAGE_EXTRACT_AND_RUN=1` (no FUSE needed); deb/rpm install the udev rules and reload udev |

The assets come from the `openrgb-1.0` pre-release of this repository (a byte-for-byte
mirror of the official files), with Codeberg as the fallback; Codeberg has refused
automated downloads before. OpenRGB is GPL-2.0-or-later and is bundled unmodified with
its licence text — see [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).

## Finding an OpenRGB

At runtime Lumen first attaches to an OpenRGB that is already listening on 6742 (e.g.
the service OpenRGB's own installer registers), otherwise starts the bundled copy as a
silent, window-less SDK server (`--server --server-host 127.0.0.1 --noautoconnect`) and
stops it again on exit; if that fails it falls back to a system-wide install.
`LUMEN_OPENRGB_PATH` overrides the executable.

## Lifecycle

Lumen prefers to own the OpenRGB process it talks to: started with Lumen, restarted if
it stops answering (after ten seconds, at most every two minutes), stopped when Lumen
quits. If an earlier Lumen was killed and left its OpenRGB running, the next Lumen
recognises the executable and takes the process over. On macOS and Linux the bundled
copy is simply a child process.

If OpenRGB goes away while it is Lumen's job to run it (it crashed, or someone ended
it), Lumen starts it again after five seconds, at most every thirty.

## Managed mode on Windows

SMBus devices (motherboard and RAM lighting) need OpenRGB to run with administrator
rights, and the OpenRGB installer's own service gets in the way of everything else: it
runs outside Lumen's control, outlives Lumen, and can wedge. Settings → OpenRGB → *Let
Lumen manage OpenRGB* replaces it with a single admin prompt
(`src-tauri/packaging/windows/openrgb-adopt.ps1`):

1. OpenRGB's own service, if installed, is stopped and disabled.
2. Lumen's bundled OpenRGB is copied to `C:\Program Files\Lumen\OpenRGB`, where only
   administrators can change it.
3. A Windows service `LumenOpenRGB` runs that copy as LocalSystem, started on demand.
   Its permissions let interactive users start and stop it, so a normal-user Lumen
   starts it with itself, restarts it when it stops answering and stops it on exit,
   without further prompts. Windows restarts it if it crashes.

In service mode OpenRGB keeps its settings in `service_config\` next to the executable;
the adopt step pins its SDK server to `127.0.0.1` there. If OpenRGB ignores a stop
request (it has been seen wedging in `STOP_PENDING`), a restart from Settings ends it
through one admin prompt.

If the service runs a different OpenRGB than the one shipped with this Lumen (an
earlier install, or a development build), Settings says so and offers *Update Lumen's
OpenRGB service*. *Hand back to the service* / *Stop managing* (`openrgb-release.ps1`)
removes the service and its folder and re-enables OpenRGB's own service if there is one.

Every elevated step runs as one PowerShell whose script travels inside its own command
line (`-EncodedCommand`), so nothing in a user-writable folder can be swapped between
Lumen writing it and the elevated shell running it.

### Lumen 0.2's scheduled task

Lumen 0.2 used a Task Scheduler task (`LumenOpenRGB`) instead of the service. It ran
OpenRGB with highest privileges straight from the per-user install folder, and loaded
OpenRGB plugins from the user's profile, so any program running as the user could swap
a file there and get administrator rights without a prompt. Lumen still recognises and
uses that task, but flags it as a security fix in the top bar and in Settings; switching
to the service removes it.
