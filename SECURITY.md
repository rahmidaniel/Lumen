# Security policy

## Supported versions

Fixes go into the next release only. Lumen updates itself (Settings → Updates), so
staying on the latest version is one click.

## Reporting a vulnerability

Please don't open a public issue. Report it privately through GitHub instead:
[**Report a vulnerability**](https://github.com/rahmidaniel/lumen/security/advisories/new)
(Security tab → *Report a vulnerability*), with the Lumen version, your OS and the steps
to reproduce. The fix ships as a normal release, and the advisory is published once
users have had a chance to update.

## What is in scope

The parts of Lumen where a bug would matter most:

- **The elevated OpenRGB task on Windows.** *Let Lumen manage OpenRGB* registers a Task
  Scheduler task (`LumenOpenRGB`) that runs OpenRGB with highest privileges for your
  account, see [`docs/openrgb.md`](docs/openrgb.md).
- **Updates.** Lumen verifies every update against the public key in
  `src-tauri/tauri.conf.json` before installing it.
- **The OpenRGB SDK port.** OpenRGB listens on `127.0.0.1:6742` without authentication,
  as OpenRGB does by design, so local programs can change your lighting. It should never
  be reachable from other machines.
- **Device access.** Battery readers only send read requests; Lumen writes to a device's
  onboard memory only when you save boot lighting.

Vulnerabilities in OpenRGB itself belong upstream, at
[codeberg.org/OpenRGB/OpenRGB](https://codeberg.org/OpenRGB/OpenRGB).
