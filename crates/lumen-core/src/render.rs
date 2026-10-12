//! Turns a scene into one colour per LED per device.

use openrgb_sdk::{Color, Device};

use crate::canvas::{build, DeviceGeometry, LedPoint, Layout};
use crate::effects::{Rgb, SampleCtx};
use crate::scene::{blend, Scene};

pub struct Renderer {
    geometry: Vec<DeviceGeometry>,
    points: Vec<LedPoint>,
    seeds: Vec<u32>,
    device_ids: Vec<u32>,
    led_counts: Vec<usize>,
}

impl Renderer {
    pub fn new(devices: &[Device], layout: &Layout) -> Self {
        let (geometry, points) = build(devices, layout);
        let seeds = points.iter().map(|p| p.device.wrapping_mul(0x0100_0193) ^ (p.led as u32).wrapping_mul(0x9E37_79B9)).collect();
        Self {
            geometry,
            points,
            seeds,
            device_ids: devices.iter().map(|d| d.id).collect(),
            led_counts: devices.iter().map(|d| d.leds.len()).collect(),
        }
    }

    pub fn geometry(&self) -> &[DeviceGeometry] {
        &self.geometry
    }

    pub fn points(&self) -> &[LedPoint] {
        &self.points
    }

    pub fn device_ids(&self) -> &[u32] {
        &self.device_ids
    }

    /// Render at time `t` into `out[device_index][led]`. `out` is resized as needed.
    pub fn render(&self, scene: &Scene, t: f64, out: &mut Vec<Vec<Color>>) {
        out.resize_with(self.led_counts.len(), Vec::new);
        for (frame, &n) in out.iter_mut().zip(&self.led_counts) {
            frame.clear();
            frame.resize(n, Color::BLACK);
        }

        let layers: Vec<_> = scene.layers.iter().filter(|l| l.enabled && l.opacity > 0.0).collect();
        for (p, &seed) in self.points.iter().zip(&self.seeds) {
            let ctx = SampleCtx { u: p.u, v: p.v, t, seed, device: p.device, led: p.led as u32 };
            let mut c = Rgb::BLACK;
            for layer in &layers {
                if layer.mask.includes(p) {
                    c = blend(c, layer.effect.sample(&ctx), layer.opacity, layer.blend);
                }
            }
            if let Some(slot) = out.get_mut(p.device_index).and_then(|f| f.get_mut(p.led)) {
                *slot = c.scale(scene.brightness).to_color();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::Effect;
    use crate::scene::{Layer, Mask};
    use openrgb_sdk::{DeviceType, Led, Zone, ZoneType};

    fn strip(id: u32, n: u32) -> Device {
        Device {
            id,
            kind: DeviceType::LEDSTRIP,
            name: format!("strip{id}"),
            vendor: String::new(),
            description: String::new(),
            version: String::new(),
            serial: String::new(),
            location: String::new(),
            modes: vec![],
            active_mode: 0,
            zones: vec![Zone {
                name: "Strip".into(),
                kind: ZoneType::LINEAR,
                leds_min: n,
                leds_max: n,
                leds_count: n,
                matrix: None,
                segments: vec![],
                flags: 0,
                active_mode: None,
                modes: vec![],
                display_name: None,
            }],
            leds: (0..n).map(|i| Led { name: format!("LED {i}"), value: i }).collect(),
            colors: vec![],
            led_display_names: vec![],
            flags: 0,
            display_name: String::new(),
            configuration: String::new(),
        }
    }

    #[test]
    fn static_layer_fills_every_led_and_masks_apply() {
        let devices = vec![strip(1, 3), strip(2, 2)];
        let mut scene = Scene { layers: vec![], ..Scene::default() };
        scene.layout.ensure_placements(&devices);
        scene.layers.push(Layer::new("base", Effect::Static { color: Rgb::new(1.0, 0.0, 0.0) }));
        let mut only_second = Layer::new("blue on 2", Effect::Static { color: Rgb::new(0.0, 0.0, 1.0) });
        only_second.mask = Mask::Devices { ids: vec![2] };
        scene.layers.push(only_second);

        let renderer = Renderer::new(&devices, &scene.layout);
        let mut out = Vec::new();
        renderer.render(&scene, 0.0, &mut out);

        assert_eq!(out, vec![vec![Color::new(255, 0, 0); 3], vec![Color::new(0, 0, 255); 2]]);
    }

    #[test]
    fn brightness_scales_output_and_render_is_deterministic() {
        let devices = vec![strip(1, 4)];
        let mut scene = Scene::default();
        scene.layout.ensure_placements(&devices);
        scene.brightness = 0.5;
        let renderer = Renderer::new(&devices, &scene.layout);

        let (mut a, mut b) = (Vec::new(), Vec::new());
        renderer.render(&scene, 1.234, &mut a);
        renderer.render(&scene, 1.234, &mut b);
        assert_eq!(a, b);
        assert!(a[0].iter().all(|c| c.r <= 128 && c.g <= 128 && c.b <= 128));
    }
}
