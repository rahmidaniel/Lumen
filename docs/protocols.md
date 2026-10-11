# Protocol notes

Reference material gathered while building Lumen. Everything here was either read from the
upstream sources linked below or verified against the hardware on the dev machine.

## OpenRGB SDK (protocol v6, OpenRGB 1.0)

Source: `Documentation/OpenRGBSDK.md` and `NetworkProtocol.h` in the OpenRGB repo
(https://gitlab.com/CalcProgrammer1/OpenRGB). Transport is TCP, default port **6742**,
all integers little-endian. Length-prefixed strings are `u16 len` (len **includes** the null
terminator) followed by the bytes; "name strings" sent to the server are plain null-terminated.
`RGBColor` is a `u32` laid out as bytes `R, G, B, 0`.

### Header (16 bytes)

| field | type |
|---|---|
| magic | `"ORGB"` |
| pkt_dev_id | u32 — controller (unique ID in v6, list index before) |
| pkt_id | u32 |
| pkt_size | u32 — payload bytes after the header |

### Handshake

1. `40 REQUEST_PROTOCOL_VERSION` → send our max version (u32); server replies with its max.
   Negotiated = min of both. No reply ⇒ assume v0.
2. `50 SET_CLIENT_NAME` → null-terminated name, no reply.
3. v6: `52 SET_CLIENT_FLAGS` (u32: bit0 RGBController, bit1 Log, bit2 Profile, bit3 Plugin,
   bit4 Settings, bit16 request-local). Server answers `53 SET_SERVER_FLAGS`, `51 SET_SERVER_NAME`.
   Local-client status is granted only from localhost; required for save/delete/upload operations.

### Controllers

- `0 REQUEST_CONTROLLER_COUNT` → v6: `u32 count` + `u32[count]` unique IDs (server order).
  Pre-v6: just the count; address by index.
- `1 REQUEST_CONTROLLER_DATA` (payload `u32 negotiated_version`) → `u32 data_size` + Device Data.
- `100 DEVICE_LIST_UPDATED` (size 0) → v6: re-request the ID list and diff; pre-v6 refetch everything.
- `10 ACK` (v6): `u32 pkt_id, u32 status` (0 OK, 1 error, 2 unsupported, 3 not allowed, 4 bad ID, 5 bad data).

### Device Data

| # | field | type | since |
|---|---|---|---|
| 1 | type | i32 | v0 |
| 2 | name | str | v0 |
| 3 | vendor | str | v1 |
| 4 | description | str | v0 |
| 5 | version | str | v0 |
| 6 | serial | str | v0 |
| 7 | location | str | v0 |
| 8 | num_modes | u16 | v0 |
| 9 | active_mode | i32 | v0 |
| 10 | modes[] | Mode | v0 |
| 11 | num_zones | u16 | v0 |
| 12 | zones[] | Zone | v0 |
| 13 | num_leds | u16 | v0 |
| 14 | leds[] | LED | v0 |
| 15 | num_colors | u16 | v0 |
| 16 | colors[] | RGBColor | v0 |
| 17 | num_led_display_names + names[] | u16 + str[] | v5 |
| 18 | controller flags | u32 | v5 |
| 19 | display_name | str | v6 |
| 20 | configuration | **u32 len** + bytes (JSON) | v6 |

**Mode**: name str · value i32 (**< v6 only**) · flags u32 · speed_min u32 · speed_max u32 ·
brightness_min u32 (v3) · brightness_max u32 (v3) · colors_min u32 · colors_max u32 · speed u32 ·
brightness u32 (v3) · direction u32 · color_mode u32 · num_colors u16 · colors[].

**Zone**: name str · type i32 · leds_min u32 · leds_max u32 · leds_count u32 · matrix_len u16 +
(if non-zero) `u32 height, u32 width, u32[h*w]` (0xFFFFFFFF = no LED) · (v4) num_segments u16 +
segments[] · (v5) zone flags u32 · (v6) zone_active_mode i32 · (v6) zone_num_modes u16 + modes[] ·
(v6) zone_display_name str.

**Segment** (v4): name str · type i32 · start_idx u32 · leds_count u32 · (v6) matrix_len u16 + matrix ·
(v6) flags u32.

**LED**: name str · value u32 (**< v6 only**).

### RGBController commands (header `pkt_dev_id` = controller)

| id | name | payload |
|---|---|---|
| 1000 | ResizeZone | `i32 zone_idx, i32 new_size` (no data_size) |
| 1050 | UpdateLEDs | `u32 data_size, u16 num_colors, RGBColor[]` |
| 1051 | UpdateZoneLEDs | `u32 data_size, u32 zone_idx, u16 num_colors, RGBColor[]` |
| 1052 | UpdateSingleLED | `i32 led_idx, RGBColor` |
| 1100 | SetCustomMode | empty |
| 1101 | UpdateMode | `u32 data_size, i32 mode_idx, Mode` |
| 1102 | SaveMode (v3) | same as UpdateMode; only meaningful when the mode has MANUAL_SAVE |
| 1103 | UpdateZoneMode (v6) | `u32 data_size, u32 zone_idx, i32 mode_idx, Mode` |

Per-LED streaming = `SetCustomMode` once (switches the device to its "Direct"/custom mode), then
`UpdateLEDs` frames.

### Profiles / settings (v2+, v6 for most)

150 GetProfileList → `u32 size, u16 n, str[]` · 151 Save / 152 Load / 153 Delete (name, null-term) ·
154 Upload JSON · 155 Download JSON · 156 GetActive · 161 ClearActive · 157–160 notifications.
250 GetSettings(key) → JSON · 251 schema · 252 Modify · 253 Set · 254 Save (local clients only).

## Razer (raw HID feature reports)

Source: OpenRazer `driver/razerchromacommon.c` (https://github.com/openrazer/openrazer) and
our own `razer-strider/strider.ps1` experiments (2026-10-08).

Report: 90-byte payload (+1 report-id byte on Windows → 91). Layout: `status, transaction_id,
remaining_packets u16, protocol_type, data_size, command_class, command_id, args[80], crc, reserved`.
CRC = XOR of bytes 2..87. Reply status: 0 new, 1 busy, 2 ok, 3 fail, 4 timeout, 5 unsupported.
Replies are read back with GetFeature; match on `transaction_id`, `command_class`, `command_id`
because other software may be talking at the same time.

| purpose | class | cmd | size | args / result |
|---|---|---|---|---|
| firmware version | 0x00 | 0x81 | 0x02 | result args[0].args[1] |
| serial | 0x00 | 0x82 | 0x16 | result ASCII args[0..22] |
| get device mode | 0x00 | 0x84 | 0x02 | args[0]: 0x00 normal, 0x03 driver |
| set device mode | 0x00 | 0x04 | 0x02 | args[0] = mode, args[1] = 0 |
| **battery level** | 0x07 | 0x80 | 0x02 | result args[1] = 0–255 (×100/255 %) |
| **charging** | 0x07 | 0x84 | 0x02 | result args[1] = 0/1 |
| idle time get/set | 0x07 | 0x83 / 0x03 | 0x02 | seconds big-endian u16, 60–900 |
| low-battery threshold get/set | 0x07 | 0x81 / 0x01 | 0x01 | 0x0C = 5 %, 0x26 = 15 %, 0x3F = 25 % |
| extended matrix effect | 0x0F | 0x02 | var | `store, led, effect, …` (see below) |
| extended matrix brightness | 0x0F | 0x04 | 0x03 | `store, led, 0–255` |
| custom frame (what Synapse streams) | 0x0F | 0x03 | var | `0, row, start, stop, rgb…` |

Storage byte: `NOSTORE = 0x00`, `VARSTORE = 0x01` (persists in device flash where supported).
Effects (0x0F/0x02): `off = 0x00`, `static = 0x01` (`…, 0, 0, 1, R, G, B`), `breath = 0x02`
(`…, 1, 0, 1, R, G, B`), `spectrum = 0x03`, `wave = 0x04` (`…, dir, 0x28, 0`), `reactive = 0x05`,
`starlight = 0x07`.

Transaction IDs are per-device and sometimes per-command (from OpenRazer's `razerkbd_driver.c`):
Strider Chroma 0x3F for everything · DeathStalker V2 Pro / Pro TKL **wireless receiver** (0x0290 / 0x0296)
0x9F for everything · the same keyboards **wired** (0x0292 / 0x0298) 0x1F for power/battery/device-mode and
0x3F for effects. On Windows the control report for the DeathStalker receiver lives on interface `MI_02`
(usage page 1, usage 2) — all other collections reject the 91-byte feature report with "Incorrect function" —
and the receiver answers with status `Timeout` (4) when the wrong transaction ID is used or the keyboard is asleep.
Wireless replies take ~5 ms (OpenRazer waits 4900 µs); Lumen polls at 1.2/3.4/5.6/7.8/10 ms before re-sending.

**Single reply buffer:** a Razer device holds one response at a time. While OpenRGB streams per-LED frames
(each frame = several `0x0F/0x03` row reports plus a `0x0F/0x02`, i.e. hundreds of feature reports per
second at 60 fps) a sidecar query's reply is overwritten before `GetFeature` can read it — the scan in
`examples/razer_tid_scan.rs` showed nothing but OpenRGB's `tid=0x9f cmd=0x03` replies. Lumen therefore asks the
engine to hold the stream to that one device (`Command::HoldDevice`) for the ~150 ms a query or flash write takes.

**Finding:** Strider Chroma keeps a VARSTORE effect across power cycles, but only if the write
happens while nothing else is streaming (quit Synapse first; Synapse also resets the device on exit).

## HyperX Pulsefire Haste 2 Wireless (03F0:0F98) — battery

Source (community reverse engineering): https://github.com/Emanuel4100/omarchy-hyperx-battery, `daemon/src/main.rs`.
Vendor interface = USB interface 2 (`HID_PHYS … input2`). Reports are unnumbered: write a 64-byte output report
`50 02 00 …` (under hidapi: prefix report ID `00`), then read input reports and scan for `51 02`; byte +2 = battery %
(0–100, discard if > 100), charging = `+4 == 0x00` (on the cable) / `+4 == 0xFF` → not charging / otherwise `+3 ∈ {1, 2}`.
The mouse also sends an unsolicited 11-byte report on plug/unplug (`+6 == 0x03`, battery at +1, power at +4).
Another project reports PID 0x0B97 for a Haste 2 variant. Implemented in `crates/lumen-vendor-hyperx`.
