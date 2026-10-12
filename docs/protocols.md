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

**Device settings** (encodings from OpenRazer `razerchromacommon.c`; which ones a product has is not documented, so
Lumen reads first and offers only what answers sensibly):

| purpose | class | cmd get / set | size | args |
|---|---|---|---|---|
| polling rate | 0x00 | 0x85 / 0x05 | 0x01 | `0x01` 1000 Hz, `0x02` 500 Hz, `0x08` 125 Hz |
| HyperPolling rate | 0x00 | 0xC0 / 0x40 | 0x02 | `[0x00 or 0x01, 8000 / Hz]` (Synapse sends both) |
| DPI | 0x04 | 0x85 / 0x05 | 0x07 | `store, x BE, y BE, 0, 0` |
| DPI stages | 0x04 | 0x86 / 0x06 | 0x26 | `store, active (1-based), count`, then 7-byte records `number, x BE, y BE, 0, 0` |
| sleep timer | 0x07 | 0x83 / 0x03 | 0x02 | seconds BE, 60–900 |
| low-battery threshold | 0x07 | 0x81 / 0x01 | 0x01 | raw, `raw × 100 / 255` %, firmware takes 0x0C–0x3F |

Verified 2026-10-12 on the DeathStalker V2 Pro TKL receiver (tid 0x9F): polling rate reads 1000 Hz and accepts 500 and
back; HyperPolling answers `Unsupported`; both DPI queries answer `Ok` with zeros (a keyboard has no sensor, so zeros
mean "absent"); sleep timer 900 s → 600 s → 900 s and threshold 0x16 (9 %, the factory value) → 0x26 → 0x16 all stick.
The Strider Chroma answers none of these sensibly.

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

**Verified on Windows (2026-10-11/12):** the receiver's `MI_02` collection uses numbered reports (0x50 is the report ID,
so the 64 bytes are written as-is; the 65-byte unnumbered framing is rejected). HyperX framing (per
https://github.com/fspy/haste2ctl): request report ID `N`, sub-command in byte 1 → reply report ID `N+1` with the same
sub-command (`50 02` → `51 02`, like `40 02` → `41 02` for brightness); `ff 01 <14-byte header> <echoed id/sub>` frames
are ACKs; `45 xx` reports are unsolicited notifications. While the mouse is awake the receiver floods the interface with
`45 47 00 ff …` / `ff 01 … 45 47` pairs (~100/s); the `ff 01 … 51 02 00 00` ACK must not be parsed as data (it reads as
"0 %, charging"). The real answer starts with `51 02`, ~20 ms after the request. Observed payload over ~80 minutes:
`62 00 19|1a 00 39 0f 01 …` — +2 = 98 constant (the percentage, as the community tools read it), +3 = 0 (charge phase),
+4 = 25→26 creeping up (looks like a temperature, not a percentage), +6..7 LE = 3896–3910 mV, moving with load (cell
voltage, probably measured after a diode drop given 98 %). A sleeping mouse (idle ≳ 1–10 min) answers nothing; keep the
last reading. Lumen shows the voltage next to the percentage so the +2 reading can be sanity-checked over time.

### Polling rate and DPI stages (`crates/lumen-vendor-hyperx/src/config.rs`)

Source: https://github.com/fspy/haste2ctl (wired Haste 2, `03F0:0B97`). `32 02` asks for the table, answered by `33 02 RR MM AA`
followed by five 5-byte stage records (`u16 LE code, R, G, B`): `RR` is a divisor (`Hz = 8000 / RR`, 0x40 = 125 … 0x01 =
8000), `MM` the bit mask of enabled stages, `AA` the active stage (0-based), DPI = `(code + 1) × 50`. A write is
`32 01 00 00` followed by bytes 2–61 of that reply, acknowledged by `ff 01 …` with bytes 14–16 = `32 01 <result>` (0 =
accepted). Writes change the live profile only.

**Verified on the wireless Haste 2 (`0F98`, through its receiver, 2026-10-12):** the same table answers on `MI_02`
(`33 02 08 0f 01 07 00 ff 00 00 0f 00 00 00 ff 1f 00 ff ff 00 3f 00 00 ff 00 07 02 ff ff ff …`: 1000 Hz, four stages
400/800/1600/3200 in red/blue/yellow/green, stage 2 active, a disabled fifth record at 26 000), and a write of 500 Hz with
three stages was accepted, read back and reverted. The receiver drops a request that arrives while it is relaying
another, so Lumen sends one vendor request at a time across all devices and asks for the table twice before giving up.
Because the mouse returns to its own profile when it is switched off, Lumen stores what the user chose
(`device-settings.json`) and writes it back when a poll finds the mouse on different values.

## Battery sidecars from published protocols (crates/lumen-vendor-hid)

None of the devices below were available while writing this (2026-10-12); the implementations follow the cited
open-source tools byte for byte, validate every reply strictly, and report "asleep" rather than guess. All queries are
read-only. The sidecar stops polling a device once its vendor module reports that it has no battery feature.

### Logitech HID++ 2.0 (046D)

Sources: Solaar `lib/logitech_receiver/{base,hidpp20,hidpp20_constants,hidpp10_constants}.py`, Linux
`drivers/hid/hid-logitech-hidpp.c`, libratbag `src/hidpp20.c`, cvuchener/hidpp `hidpp/Dispatcher.cpp`, OpenRGB
`LogitechProtocolCommon.cpp`.

Long report `11 <index> <feature> <fn<<4 | sw id> <16 params>`; index `FF` for a cable device (Bluetooth answers with `00`),
1–7 for receiver slots (Lightspeed C539/C53A/C53D/C53F/C541/C543/C545/C547/C54D, Unifying C52B/C532/C534). Replies repeat
bytes 2–3 and the software id (Lumen uses 0x0C; kernel 1, OpenRGB 7, libratbag 8, Solaar 0xB). Errors: byte 2 `FF`
(HID++ 2.0: 0x08 busy, 0x06/0x07/0x09 unsupported) or `8F` (HID++ 1.0, sent by receivers: 0x08 nothing paired, 0x09 resource
error = asleep/out of range, 0x07 busy, 0x01 = HID++ 1.0 device), code in byte 5. Windows splits the short (0x10, usage 1)
and long (0x11, usage 2) collections of page 0xFF00 (page 0xFF43 keeps the flag in the usage's low byte) into separate HID
nodes; Lumen writes long frames to the long node and reads every HID++ node, since receivers answer errors on the short one.
IRoot: ping `00 1S 00 00 5A` (fn 1, echo of 0x5A), getFeature `00 0S <id hi> <id lo>` → feature index (0 = absent).
0x1004 UNIFIED_BATTERY getCapabilities (fn 0: byte +1 bit 1 = state-of-charge supported) and getStatus (fn 1: +0 percent,
+1 level bits 1/2/4/8 critical/low/good/full, +2 charging status 0 none, 1 charging, 2 slow, 3 full, 4 error, +3 external
power). 0x1000 BATTERY_STATUS fn 0: +0 level %, +1 next level, +2 status (0 discharging, 1/2/4 charging, 3 full; level is 0
while charging). 0x1001 BATTERY_VOLTAGE fn 0: +0..1 mV big-endian, +2 flags (bit 7 external power; bits 0–2 charge status
0 charging / 1 full / 2 stopped / 7 error, valid only with bit 7; bit 3 fast, bit 4 slow); percent from Solaar's 13-point
discharge curve (4186 mV = 100 % … 3500 mV = 0 %).

**Settings** (Solaar `settings_templates.py`, untested): 0x2201 ADJUSTABLE_DPI — fn 1 list (`sensor, 0, page`; after the
sensor byte, u16 BE values ending in 0, a value with the top three bits set is a step `& 0x1FFF` up to the next value),
fn 2 current (`sensor, dpi, default`; 0 = default), fn 3 set (`sensor, dpi`). 0x2202 EXTENDED_ADJUSTABLE_DPI — fn 1
capabilities (byte 2: bit 0 separate Y, bit 1 lift-off), fn 2 list after three bytes, fn 5 read (`sensor, x, default x,
y, default y, lod`), fn 6 write (`sensor, x, y[, lod]`). 0x8060 REPORT_RATE — fn 0 flags (bit i = (i + 1) ms), fn 1
current ms, fn 2 set. 0x8061 EXTENDED_ADJUSTABLE_REPORT_RATE — fn 1 u16 flags (bit i = 8, 4, 2, 1 ms, 500, 250, 125 µs),
fn 2 current code, fn 3 set. 0x8100 ONBOARD_PROFILES — fn 2 reads 1 while an onboard profile overrides DPI and report
rate, fn 1 sets 1 (onboard) or 2 (host). Lumen applies settings to the first receiver slot whose device has any of
these features.

### Glorious CORE v1 wireless mice (258A)

Sources: korkje/mxw `src/report/battery.rs`, dkbednarczyk/mxw `src/util/status.rs`, AwesomeTy18/GloriousBatteryMonitor
`Mice/MouseV2ProBatteryChecker.cs`, OpenMouse `src/glorious-classic/index.ts`.

Cable / dongle PIDs: Model O Wireless 2011/2022, O- 2013/2024, D 2012/2023, D- 2014/2025, O Pro 2015/2027, Series One Pro
2018/2031, O 2 PRO 2019/2033, D 2 PRO 201A/2034, Series 2 PRO 201D/2039, O 2 PRO 4K/8K 201B/2035, D 2 PRO 4K/8K 201C/2036.
Interface 2, vendor collection usage page 0xFF01 (0xFF00 older firmware, 0xFFFF on the 4K/8K PRO). Unnumbered 64-byte
**feature** report `00 00 02 02 00 83` (+2 target mouse, +3 length, +4 bank 0, +5 register 0x83 = battery; 0x81 = firmware);
wait ~60 ms; `GetFeature` the reply: +1 status (`A1` awake, `A4` asleep, `A0` waking, `A2` failed), +6 must echo `83`
(else stale), +7 charging flag, +8 percent (tools show 0 as 1 %). The dongle answers with `A4`/`A0` while the mouse is off.
The Pixart "Model x 2 Wireless" family (093A:821A…826D) cannot be queried: it pushes `06 FB <percent> <toggle>` input
reports on interface 1 every 20–40 s over the dongle only, and the "charging" toggle is meaningless — not implemented.

### ASUS ROG wireless mice (0B05)

Sources: seerge/g-helper `app/Peripherals/Mouse/AsusMouse.cs` (`GetBatteryReportPacket`, `ParseBattery`,
`ParseChargingState`, `IsMouseError`), Humblemonk/rogctl `src/protocol.rs` (captured vector), OpenMouse `src/asus/`.

Cable / dongle PIDs: Keris Wireless 195E/1960 (level 0–4 ×25), Keris Wireless AimPoint 1A66/1A68, Gladius III Wireless
197D/197F, Gladius III AimPoint 1A70/1A72, Chakram 18E3/18E5 (level ×25), Chakram X 1A18/1A1A, Spatha X 1977/1979, Harpe Ace
Aim Lab 1A92/1A94 (all others percent). Interface 0, usage page 0xFF01, unnumbered 64-byte reports. Request `12 07 00 …`.
Drain queued input first (`12 01` button bitmaps share the collection), then read until a report echoes `12 07`: +4 battery,
+5 auto power-off, +6 low-battery warning, +7..8 LE millivolts (one tool; unverified), +9 charging ≠ 0 — offsets without the
report-ID byte. `FF AA` = request rejected (asleep / out of range); all zeros = empty packet (mouse off behind the dongle);
0 % and not charging = dongle answering for an absent mouse. The ROG Omni (1ACE) and SpeedNova (1AD0) receivers use report
ID 3, shifting everything by one byte; the parser accepts that framing but those receivers are not in the product list yet.
Captured (rogctl, Harpe II Ace on SpeedNova): `03 12 07 00 00 50 02 14 d8 0f 00 00 01` → 80 %, 4056 mV, not charging.

### SteelSeries wireless mice (1038)

Source: flozz/rivalcfg `rivalcfg/devices/*.py` (`battery_level` commands), `rivalcfg/mouse.py` `Mouse.battery`,
`rivalcfg/usbhid.py`; a third-party Aerox 3 Wireless capture for the echo bytes.

Aerox 3/5/9 Wireless (183A/1838, 187A/1878, 1854/1852, 185E/185C, 1862/1860, 185A/1858, 1876/1874) and Prime (Mini)
Wireless (1842/1840, 184A/1848), cable/dongle: interface 3 (usage page 0xFFC0), output report `92` on the cable PID,
`D2` (= `92 | 0x40`) on the dongle. Reply +1: bit 7 charging, `((b & 0x7F) − 1) × 5` percent; `FF` = mouse off/asleep (the
dongle answers `40 FF`). Observed echo byte `12` for the cable query. rivalcfg issue #250: the query "only works sometimes"
on Windows, so Lumen retries three times. Rival 3 Wireless 1830 / Gen 2 1872 (interface 3) and Rival 650 172B/1726
(interface 0): output report `AA 01`; reply +0 percent, +2 charging; the Rival 650 dongle is silent while the mouse is off.

### Corsair wireless mice and keyboards (1B1C)

Sources: ckb-next `src/daemon/device.c` (`nxp_get_battery_info`), `device_bragi.c` (`bragi_get_battery_info`),
`bragi_common.c`, `bragi_proto.h`, `nxp_proto.h`, `usb.h` (model tables); OpenRGB `CorsairPeripheralV2Controller`
(interface/usage page, retry-on-zero) and `CorsairPeripheralController` (feature-report fallback).

Bragi ("V2", interface 1, usage page 0xFF42): Dark Core RGB Pro 1B80/1B81, Pro SE 1B7E/1B7F, Ironclaw RGB Wireless
1B4C/1B66, Harpoon RGB Wireless 1B5E/1B65, K57 RGB Wireless 1B6E/1B62, Slipstream receiver 1BA6. Property read
`(08 | id) 02 <property>`, id 0 on the cable, 9 (child 1) through a receiver; reply `(08 | id) 02 <error> <value LE24>`.
0x0F = battery level in 0.1 % (÷10), 0x10 = status (1 charging, 2 discharging, 3 full). A receiver answers 0 while its
device sleeps; OpenRGB polls up to 8× / 120 ms before believing it.
NXP (legacy, interface 1, usage page 0xFFC2): Dark Core RGB 1B35/1B64, K63 Wireless 1B45/1B8C/1B50/1B8F. Request `0E 50`;
reply echoes `0E 50`, +4 level index 0–4 → {0, 15, 30, 50, 100} %, +5 status (1 discharging, 2 charging, 3 full). Firmware
before 1.20 answers only on the feature-report path (same bytes via `SetFeature`/`GetFeature`, payload at +1).
Dark Core RGB SE 1B4B/1B51 is Bragi in OpenRGB and NXP in ckb-next; Lumen tries both. K100 AIR, Katar Pro Wireless and
Sabre RGB Pro Wireless have no published PIDs or protocol and are not handled.
