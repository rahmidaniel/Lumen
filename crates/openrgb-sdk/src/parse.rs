//! Serialization of controller descriptions and command payloads.
//!
//! Field order follows `RGBController::GetDeviceDescriptionData` /
//! `SetModeDescription` in OpenRGB, with the `protocol_version` conditionals
//! those functions apply.

use crate::error::{Error, Result};
use crate::model::*;
use crate::wire::{Reader, Writer};

/// Parse a `REQUEST_CONTROLLER_DATA` response. The payload starts with a
/// `u32 data_size` followed by the device description.
pub fn parse_device(payload: &[u8], id: u32, version: u32) -> Result<Device> {
    let mut r = Reader::new(payload);
    let declared = r.u32()? as usize;
    if declared != payload.len() {
        tracing::debug!(declared, actual = payload.len(), "device data_size mismatch; parsing anyway");
    }
    read_device(&mut r, id, version)
}

pub fn read_device(r: &mut Reader<'_>, id: u32, version: u32) -> Result<Device> {
    let kind = DeviceType(r.i32()?);
    let name = r.str16()?;
    let vendor = if version >= 1 { r.str16()? } else { String::new() };
    let description = r.str16()?;
    let version_str = r.str16()?;
    let serial = r.str16()?;
    let location = r.str16()?;

    let num_modes = r.u16()?;
    let active_mode = r.i32()?;
    let mut modes = Vec::with_capacity(num_modes as usize);
    for _ in 0..num_modes {
        modes.push(read_mode(r, version)?);
    }

    let num_zones = r.u16()?;
    let mut zones = Vec::with_capacity(num_zones as usize);
    for _ in 0..num_zones {
        zones.push(read_zone(r, version)?);
    }

    let num_leds = r.u16()?;
    let mut leds = Vec::with_capacity(num_leds as usize);
    for _ in 0..num_leds {
        leds.push(read_led(r, version)?);
    }

    let colors = read_colors(r)?;

    let mut led_display_names = Vec::new();
    let mut flags = 0;
    if version >= 5 {
        let n = r.u16()?;
        led_display_names.reserve(n as usize);
        for _ in 0..n {
            led_display_names.push(r.str16()?);
        }
        flags = r.u32()?;
    }

    let (display_name, configuration) =
        if version >= 6 { (r.str16()?, r.str32()?) } else { (name.clone(), String::new()) };

    Ok(Device {
        id,
        kind,
        name,
        vendor,
        description,
        version: version_str,
        serial,
        location,
        modes,
        active_mode,
        zones,
        leds,
        colors,
        led_display_names,
        flags,
        display_name,
        configuration,
    })
}

pub fn read_mode(r: &mut Reader<'_>, version: u32) -> Result<Mode> {
    let name = r.str16()?;
    let value = if version < 6 { r.i32()? } else { 0 };
    let flags = ModeFlags(r.u32()?);
    let speed_min = r.u32()?;
    let speed_max = r.u32()?;
    let (brightness_min, brightness_max) = if version >= 3 { (r.u32()?, r.u32()?) } else { (0, 0) };
    let colors_min = r.u32()?;
    let colors_max = r.u32()?;
    let speed = r.u32()?;
    let brightness = if version >= 3 { r.u32()? } else { 0 };
    let direction = Direction(r.u32()?);
    let color_mode = ColorMode(r.u32()?);
    let colors = read_colors(r)?;
    Ok(Mode {
        name,
        value,
        flags,
        speed_min,
        speed_max,
        brightness_min,
        brightness_max,
        colors_min,
        colors_max,
        speed,
        brightness,
        direction,
        color_mode,
        colors,
    })
}

/// Mirror of [`read_mode`], used for `UpdateMode` / `SaveMode` payloads.
pub fn write_mode(w: &mut Writer, mode: &Mode, version: u32) {
    w.str16(&mode.name);
    if version < 6 {
        w.i32(mode.value);
    }
    w.u32(mode.flags.0).u32(mode.speed_min).u32(mode.speed_max);
    if version >= 3 {
        w.u32(mode.brightness_min).u32(mode.brightness_max);
    }
    w.u32(mode.colors_min).u32(mode.colors_max).u32(mode.speed);
    if version >= 3 {
        w.u32(mode.brightness);
    }
    w.u32(mode.direction.0).u32(mode.color_mode.0);
    write_colors(w, &mode.colors);
}

fn read_zone(r: &mut Reader<'_>, version: u32) -> Result<Zone> {
    let name = r.str16()?;
    let kind = ZoneType(r.i32()?);
    let leds_min = r.u32()?;
    let leds_max = r.u32()?;
    let leds_count = r.u32()?;
    let matrix = read_matrix(r)?;

    let mut segments = Vec::new();
    if version >= 4 {
        let n = r.u16()?;
        segments.reserve(n as usize);
        for _ in 0..n {
            segments.push(read_segment(r, version)?);
        }
    }

    let flags = if version >= 5 { r.u32()? } else { 0 };

    let (active_mode, modes, display_name) = if version >= 6 {
        let active = r.i32()?;
        let n = r.u16()?;
        let mut modes = Vec::with_capacity(n as usize);
        for _ in 0..n {
            modes.push(read_mode(r, version)?);
        }
        (Some(active), modes, Some(r.str16()?))
    } else {
        (None, Vec::new(), None)
    };

    Ok(Zone { name, kind, leds_min, leds_max, leds_count, matrix, segments, flags, active_mode, modes, display_name })
}

fn read_segment(r: &mut Reader<'_>, version: u32) -> Result<Segment> {
    let name = r.str16()?;
    let kind = ZoneType(r.i32()?);
    let start_idx = r.u32()?;
    let leds_count = r.u32()?;
    let (matrix, flags) = if version >= 6 { (read_matrix(r)?, r.u32()?) } else { (None, 0) };
    Ok(Segment { name, kind, start_idx, leds_count, matrix, flags })
}

/// `u16 byte_len` then, if non-zero, `u32 height, u32 width, u32[height*width]`.
fn read_matrix(r: &mut Reader<'_>) -> Result<Option<MatrixMap>> {
    let len = r.u16()? as usize;
    if len == 0 {
        return Ok(None);
    }
    let mut m = Reader::new(r.bytes(len)?);
    let height = m.u32()?;
    let width = m.u32()?;
    let cells = (height as usize)
        .checked_mul(width as usize)
        .ok_or_else(|| Error::Protocol(format!("matrix {width}x{height} overflows")))?;
    let mut map = Vec::with_capacity(cells);
    for _ in 0..cells {
        map.push(m.u32()?);
    }
    Ok(Some(MatrixMap { height, width, map }))
}

fn read_led(r: &mut Reader<'_>, version: u32) -> Result<Led> {
    let name = r.str16()?;
    let value = if version < 6 { r.u32()? } else { 0 };
    Ok(Led { name, value })
}

fn read_colors(r: &mut Reader<'_>) -> Result<Vec<Color>> {
    let n = r.u16()? as usize;
    let mut colors = Vec::with_capacity(n);
    for _ in 0..n {
        colors.push(Color::from_u32(r.u32()?));
    }
    Ok(colors)
}

fn write_colors(w: &mut Writer, colors: &[Color]) {
    w.u16(colors.len() as u16);
    for c in colors {
        w.u32(c.to_u32());
    }
}

fn check_color_count(colors: &[Color]) -> Result<()> {
    if colors.len() > u16::MAX as usize {
        return Err(Error::TooLarge(colors.len()));
    }
    Ok(())
}

/// `REQUEST_CONTROLLER_COUNT` response → controller IDs (v6) or `0..count` (older).
pub fn parse_controller_ids(payload: &[u8], version: u32) -> Result<Vec<u32>> {
    let mut r = Reader::new(payload);
    let count = r.u32()?;
    if version >= 6 {
        let mut ids = Vec::with_capacity(count as usize);
        for _ in 0..count {
            ids.push(r.u32()?);
        }
        Ok(ids)
    } else {
        Ok((0..count).collect())
    }
}

/// `PROFILEMANAGER_GET_PROFILE_LIST` response: `u32 data_size, u16 n, str16[n]`.
pub fn parse_profile_list(payload: &[u8]) -> Result<Vec<String>> {
    let mut r = Reader::new(payload);
    let _data_size = r.u32()?;
    let n = r.u16()?;
    let mut names = Vec::with_capacity(n as usize);
    for _ in 0..n {
        names.push(r.str16()?);
    }
    Ok(names)
}

/// `UPDATELEDS`: `u32 data_size, u16 num_colors, RGBColor[]`.
pub fn encode_update_leds(colors: &[Color]) -> Result<Vec<u8>> {
    check_color_count(colors)?;
    let mut w = Writer::with_capacity(6 + colors.len() * 4);
    w.u32(0);
    write_colors(&mut w, colors);
    let len = w.len() as u32;
    w.patch_u32(0, len);
    Ok(w.finish())
}

/// `UPDATEZONELEDS`: `u32 data_size, u32 zone_idx, u16 num_colors, RGBColor[]`.
pub fn encode_update_zone_leds(zone_idx: u32, colors: &[Color]) -> Result<Vec<u8>> {
    check_color_count(colors)?;
    let mut w = Writer::with_capacity(10 + colors.len() * 4);
    w.u32(0).u32(zone_idx);
    write_colors(&mut w, colors);
    let len = w.len() as u32;
    w.patch_u32(0, len);
    Ok(w.finish())
}

/// `UPDATESINGLELED`: `i32 led_idx, RGBColor`.
pub fn encode_update_single_led(led_idx: i32, color: Color) -> Vec<u8> {
    let mut w = Writer::with_capacity(8);
    w.i32(led_idx).u32(color.to_u32());
    w.finish()
}

/// `UPDATEMODE` / `SAVEMODE`: `u32 data_size, i32 mode_idx, Mode`.
pub fn encode_update_mode(mode_idx: i32, mode: &Mode, version: u32) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(0).i32(mode_idx);
    write_mode(&mut w, mode, version);
    let len = w.len() as u32;
    w.patch_u32(0, len);
    w.finish()
}

/// `UPDATEZONEMODE` (v6): `u32 data_size, u32 zone_idx, i32 mode_idx, Mode`.
pub fn encode_update_zone_mode(zone_idx: u32, mode_idx: i32, mode: &Mode, version: u32) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(0).u32(zone_idx).i32(mode_idx);
    write_mode(&mut w, mode, version);
    let len = w.len() as u32;
    w.patch_u32(0, len);
    w.finish()
}

/// `RESIZEZONE`: `i32 zone_idx, i32 new_size` (no size prefix).
pub fn encode_resize_zone(zone_idx: i32, new_size: i32) -> Vec<u8> {
    let mut w = Writer::with_capacity(8);
    w.i32(zone_idx).i32(new_size);
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize a device the way the server does, so the parser can be tested
    /// without a live OpenRGB. Mirrors `read_device` field for field.
    fn write_device(w: &mut Writer, d: &Device, version: u32) {
        w.i32(d.kind.0).str16(&d.name);
        if version >= 1 {
            w.str16(&d.vendor);
        }
        w.str16(&d.description).str16(&d.version).str16(&d.serial).str16(&d.location);
        w.u16(d.modes.len() as u16).i32(d.active_mode);
        for m in &d.modes {
            write_mode(w, m, version);
        }
        w.u16(d.zones.len() as u16);
        for z in &d.zones {
            w.str16(&z.name).i32(z.kind.0).u32(z.leds_min).u32(z.leds_max).u32(z.leds_count);
            write_matrix(w, z.matrix.as_ref());
            if version >= 4 {
                w.u16(z.segments.len() as u16);
                for s in &z.segments {
                    w.str16(&s.name).i32(s.kind.0).u32(s.start_idx).u32(s.leds_count);
                    if version >= 6 {
                        write_matrix(w, s.matrix.as_ref());
                        w.u32(s.flags);
                    }
                }
            }
            if version >= 5 {
                w.u32(z.flags);
            }
            if version >= 6 {
                w.i32(z.active_mode.unwrap_or(-1)).u16(z.modes.len() as u16);
                for m in &z.modes {
                    write_mode(w, m, version);
                }
                w.str16(z.display_name.as_deref().unwrap_or(""));
            }
        }
        w.u16(d.leds.len() as u16);
        for l in &d.leds {
            w.str16(&l.name);
            if version < 6 {
                w.u32(l.value);
            }
        }
        write_colors(w, &d.colors);
        if version >= 5 {
            w.u16(d.led_display_names.len() as u16);
            for n in &d.led_display_names {
                w.str16(n);
            }
            w.u32(d.flags);
        }
        if version >= 6 {
            w.str16(&d.display_name).str32(&d.configuration);
        }
    }

    fn write_matrix(w: &mut Writer, m: Option<&MatrixMap>) {
        match m {
            None => {
                w.u16(0);
            }
            Some(m) => {
                w.u16((8 + 4 * m.map.len()) as u16).u32(m.height).u32(m.width);
                for v in &m.map {
                    w.u32(*v);
                }
            }
        }
    }

    fn sample_mode(name: &str, flags: u32) -> Mode {
        Mode {
            name: name.into(),
            value: 7,
            flags: ModeFlags(flags),
            speed_min: 1,
            speed_max: 10,
            brightness_min: 0,
            brightness_max: 255,
            colors_min: 0,
            colors_max: 2,
            speed: 5,
            brightness: 200,
            direction: Direction::RIGHT,
            color_mode: ColorMode::PER_LED,
            colors: vec![Color::new(1, 2, 3)],
        }
    }

    fn sample_device(version: u32) -> Device {
        let matrix = MatrixMap { height: 1, width: 3, map: vec![0, MatrixMap::NO_LED, 1] };
        let zone_modes = if version >= 6 { vec![sample_mode("Zone Static", 1 << 6)] } else { vec![] };
        Device {
            id: 0xC0FFEE,
            kind: DeviceType::MOUSEMAT,
            name: "Razer Strider Chroma".into(),
            vendor: if version >= 1 { "Razer".into() } else { String::new() },
            description: "Razer Device".into(),
            version: "1.0".into(),
            serial: "ST2327".into(),
            location: "HID: \\\\?\\hid#vid_1532".into(),
            modes: vec![sample_mode("Direct", 1 << 5), sample_mode("Spectrum Cycle", 1 << 8)],
            active_mode: 0,
            zones: vec![Zone {
                name: "Mat".into(),
                kind: ZoneType::MATRIX,
                leds_min: 2,
                leds_max: 2,
                leds_count: 2,
                matrix: Some(matrix.clone()),
                segments: if version >= 4 {
                    vec![Segment {
                        name: "Left".into(),
                        kind: ZoneType::LINEAR,
                        start_idx: 0,
                        leds_count: 1,
                        matrix: if version >= 6 { Some(matrix) } else { None },
                        flags: if version >= 6 { 3 } else { 0 },
                    }]
                } else {
                    vec![]
                },
                flags: if version >= 5 { 1 } else { 0 },
                active_mode: (version >= 6).then_some(0),
                modes: zone_modes,
                display_name: (version >= 6).then(|| "Mat".to_string()),
            }],
            leds: vec![Led { name: "LED 0".into(), value: 0 }, Led { name: "LED 1".into(), value: 1 }],
            colors: vec![Color::new(10, 20, 30), Color::new(40, 50, 60)],
            led_display_names: if version >= 5 { vec!["Left".into(), "Right".into()] } else { vec![] },
            flags: if version >= 5 { 0x10 } else { 0 },
            display_name: if version >= 6 { "Desk mat".into() } else { "Razer Strider Chroma".into() },
            configuration: if version >= 6 { "{\"x\":1}".into() } else { String::new() },
        }
    }

    fn normalize_for(version: u32, mut d: Device) -> Device {
        // Fields that are not on the wire for `version` come back as defaults.
        if version >= 6 {
            for m in d.modes.iter_mut().chain(d.zones.iter_mut().flat_map(|z| z.modes.iter_mut())) {
                m.value = 0;
            }
            for l in &mut d.leds {
                l.value = 0;
            }
        }
        if version < 3 {
            for m in &mut d.modes {
                m.brightness_min = 0;
                m.brightness_max = 0;
                m.brightness = 0;
            }
        }
        d
    }

    #[test]
    fn device_roundtrip_every_protocol_version() {
        for version in 0..=6 {
            let device = sample_device(version);
            let mut w = Writer::new();
            w.u32(0);
            write_device(&mut w, &device, version);
            let len = w.len() as u32;
            w.patch_u32(0, len);
            let payload = w.finish();

            let parsed = parse_device(&payload, device.id, version).unwrap_or_else(|e| panic!("v{version}: {e}"));
            assert_eq!(parsed, normalize_for(version, device), "protocol v{version}");
        }
    }

    #[test]
    fn device_helpers() {
        let d = sample_device(6);
        assert_eq!(d.direct_mode_index(), Some(0));
        assert_eq!(d.saveable_modes().map(|(i, _)| i).collect::<Vec<_>>(), vec![1]);
        assert_eq!(d.zone_led_offset(1), 2);
    }

    #[test]
    fn update_leds_payload_layout() {
        let bytes = encode_update_leds(&[Color::new(1, 2, 3), Color::new(4, 5, 6)]).unwrap();
        assert_eq!(bytes, vec![14, 0, 0, 0, 2, 0, 1, 2, 3, 0, 4, 5, 6, 0]);
    }

    #[test]
    fn update_zone_leds_payload_layout() {
        let bytes = encode_update_zone_leds(1, &[Color::new(9, 8, 7)]).unwrap();
        assert_eq!(bytes, vec![14, 0, 0, 0, 1, 0, 0, 0, 1, 0, 9, 8, 7, 0]);
    }

    #[test]
    fn update_mode_payload_roundtrips() {
        let mode = sample_mode("Breathing", 1 << 0 | 1 << 8);
        for version in [3u32, 5, 6] {
            let bytes = encode_update_mode(2, &mode, version);
            let mut r = Reader::new(&bytes);
            assert_eq!(r.u32().unwrap() as usize, bytes.len());
            assert_eq!(r.i32().unwrap(), 2);
            let parsed = read_mode(&mut r, version).unwrap();
            let mut expected = mode.clone();
            if version >= 6 {
                expected.value = 0;
            }
            assert_eq!(parsed, expected);
            assert_eq!(r.remaining(), 0);
        }
    }

    #[test]
    fn controller_ids_by_version() {
        let mut w = Writer::new();
        w.u32(2).u32(0xAAAA).u32(0xBBBB);
        let payload = w.finish();
        assert_eq!(parse_controller_ids(&payload, 6).unwrap(), vec![0xAAAA, 0xBBBB]);
        assert_eq!(parse_controller_ids(&2u32.to_le_bytes(), 5).unwrap(), vec![0, 1]);
    }

    #[test]
    fn profile_list_parses() {
        let mut w = Writer::new();
        w.u32(0).u16(2).str16("Work").str16("Game");
        let payload = w.finish();
        assert_eq!(parse_profile_list(&payload).unwrap(), vec!["Work", "Game"]);
    }

    #[test]
    fn too_many_colors_is_an_error() {
        let colors = vec![Color::BLACK; u16::MAX as usize + 1];
        assert!(matches!(encode_update_leds(&colors), Err(Error::TooLarge(_))));
    }
}
