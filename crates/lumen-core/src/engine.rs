//! The engine task: owns the OpenRGB connection, renders at a fixed rate and
//! streams changed frames. Everything else talks to it through [`EngineHandle`].

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openrgb_sdk::{Client, Color, Device, DeviceType, Event, ZoneType, DEFAULT_PORT};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, watch};

use crate::canvas::{DeviceGeometry, LedPoint, Layout};
use crate::render::Renderer;
use crate::scene::{Layer, Scene};
use crate::Result;

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub addr: SocketAddr,
    pub client_name: String,
    /// Render/stream rate.
    pub fps: u32,
    /// Rate at which preview frames are published to subscribers.
    pub preview_fps: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
            client_name: "Lumen".into(),
            fps: 60,
            preview_fps: 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    /// 0–100.
    pub percent: u8,
    pub charging: bool,
    /// Cell voltage when the device reports one (HyperX does).
    #[serde(default)]
    pub millivolts: Option<u16>,
}

/// How a device can keep a power-on effect in its own memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Onboard {
    /// Written by the Razer sidecar (VARSTORE).
    Razer,
    /// Via OpenRGB `SaveMode` on a mode flagged `MANUAL_SAVE`.
    OpenRgb,
}

/// Facts about a device that come from vendor sidecars rather than OpenRGB.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceExtras {
    pub battery: Option<Battery>,
    pub onboard: Option<Onboard>,
    /// A sidecar can read and change device settings (polling rate, DPI, sleep timer, …).
    #[serde(default)]
    pub settings: bool,
}

/// Why every LED is dark right now. While one applies the engine sends black
/// once and then stays quiet; the scene itself is untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blackout {
    /// The user switched the lights off.
    User,
    /// The PC is going to sleep or shutting down.
    Sleep,
    /// The display turned off.
    Display,
    /// The session is locked.
    Lock,
    /// Nobody touched the PC for the configured idle time.
    Idle,
}

/// How long an empty device list counts as "still detecting" after connecting
/// (OpenRGB adds devices one by one while its detection runs).
const DETECTION_GRACE: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub enum Command {
    SetScene(Scene),
    SetLayers(Vec<Layer>),
    SetLayout(Layout),
    SetBrightness(f32),
    /// Attach sidecar-provided facts (battery, onboard memory) to a device.
    SetExtras(u32, DeviceExtras),
    /// Stop streaming to one device (it keeps its last frame) while a vendor
    /// sidecar talks to it; Razer devices have a single reply buffer that a
    /// 60 fps stream would otherwise overwrite before the sidecar can read it.
    HoldDevice(u32, bool),
    /// Stop streaming to every device.
    Pause,
    Resume,
    /// Turn every LED off (`Some`) or back on (`None`).
    SetBlackout(Option<Blackout>),
    RefreshDevices,
    /// Change the render/stream rate (clamped to 15–120).
    SetFps(u32),
    Shutdown,
}

#[derive(Clone, Debug, Serialize)]
pub struct ZoneInfo {
    pub name: String,
    pub kind: ZoneType,
    pub led_count: u32,
    /// `(width, height)` for matrix zones.
    pub matrix: Option<(u32, u32)>,
}

/// UI-facing view of a device (no per-LED data).
#[derive(Clone, Debug, Serialize)]
pub struct DeviceInfo {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub kind: DeviceType,
    pub kind_label: &'static str,
    pub vendor: String,
    pub serial: String,
    pub location: String,
    pub led_count: usize,
    pub zones: Vec<ZoneInfo>,
    /// Names of modes the device can persist via OpenRGB `SaveMode`.
    pub saveable_modes: Vec<String>,
    /// Whether the device offers a per-LED mode (and so can be streamed).
    pub streamable: bool,
    pub battery: Option<Battery>,
    pub onboard: Option<Onboard>,
    /// Lumen can read and change this device's own settings.
    pub configurable: bool,
}

impl DeviceInfo {
    pub fn from_device(d: &Device, extras: Option<&DeviceExtras>) -> Self {
        let saveable_modes: Vec<String> = d.saveable_modes().map(|(_, m)| m.name.clone()).collect();
        let onboard = extras
            .and_then(|e| e.onboard)
            .or_else(|| (!saveable_modes.is_empty()).then_some(Onboard::OpenRgb));
        Self {
            id: d.id,
            name: d.name.clone(),
            display_name: if d.display_name.is_empty() { d.name.clone() } else { d.display_name.clone() },
            kind: d.kind,
            kind_label: d.kind.label(),
            vendor: d.vendor.clone(),
            serial: d.serial.clone(),
            location: d.location.clone(),
            led_count: d.leds.len(),
            zones: d
                .zones
                .iter()
                .map(|z| ZoneInfo {
                    name: z.name.clone(),
                    kind: z.kind,
                    led_count: z.leds_count,
                    matrix: z.matrix.as_ref().map(|m| (m.width, m.height)),
                })
                .collect(),
            saveable_modes,
            streamable: d.direct_mode_index().is_some(),
            battery: extras.and_then(|e| e.battery),
            onboard,
            configurable: extras.is_some_and(|e| e.settings),
        }
    }
}

/// A user-facing explanation for a failed connection attempt, when the cause
/// is more than "OpenRGB isn't running yet".
fn connection_problem(e: &crate::Error) -> Option<String> {
    match e {
        crate::Error::OpenRgb(openrgb_sdk::Error::Timeout) => {
            Some("OpenRGB is running but not responding. Restart the OpenRGB service (or OpenRGB itself) to recover.".to_string())
        }
        crate::Error::OpenRgb(openrgb_sdk::Error::Io(_)) | crate::Error::OpenRgb(openrgb_sdk::Error::Disconnected) => None,
        other => Some(format!("OpenRGB answered unexpectedly: {other}")),
    }
}

/// Everything the UI needs that changes rarely.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Snapshot {
    pub connected: bool,
    /// Why there is no connection, when the cause is more than "not running yet".
    pub problem: Option<String>,
    pub paused: bool,
    /// Set while the lights are off, with the reason.
    pub blackout: Option<Blackout>,
    /// Connected, no devices yet, and OpenRGB may still be detecting them.
    pub detecting: bool,
    /// Render/stream rate.
    pub fps: u32,
    pub devices: Vec<DeviceInfo>,
    /// Body + LED glyphs per placed device, in local millimetres.
    pub geometry: Vec<DeviceGeometry>,
    pub points: Vec<LedPoint>,
    pub scene: Scene,
}

/// One rendered frame for every device, for live preview.
#[derive(Clone, Debug)]
pub struct FrameSet {
    pub t: f64,
    pub device_ids: Vec<u32>,
    pub frames: Vec<Vec<Color>>,
}

#[derive(Clone)]
pub struct EngineHandle {
    cmd: mpsc::Sender<Command>,
    snapshot: watch::Receiver<Arc<Snapshot>>,
    frames: broadcast::Sender<Arc<FrameSet>>,
}

impl EngineHandle {
    pub async fn send(&self, cmd: Command) -> bool {
        self.cmd.send(cmd).await.is_ok()
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.borrow().clone()
    }

    pub fn watch_snapshot(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshot.clone()
    }

    pub fn subscribe_frames(&self) -> broadcast::Receiver<Arc<FrameSet>> {
        self.frames.subscribe()
    }
}

/// Start the engine on the current Tokio runtime.
pub fn spawn(config: EngineConfig, scene: Scene) -> EngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(Snapshot { fps: config.fps, scene: scene.clone(), ..Default::default() }));
    let (frames_tx, _) = broadcast::channel(4);
    let handle = EngineHandle { cmd: cmd_tx, snapshot: snapshot_rx, frames: frames_tx.clone() };

    let engine = Engine {
        config,
        scene,
        client: None,
        problem: None,
        devices: Vec::new(),
        extras: HashMap::new(),
        held: HashSet::new(),
        renderer: Renderer::new(&[], &Layout::default()),
        last: Vec::new(),
        out: Vec::new(),
        paused: false,
        blackout: None,
        force_frame: false,
        dirty: true,
        fps_changed: false,
        start: Instant::now(),
        last_preview: Instant::now(),
        reload_at: None,
        detect_since: None,
        detection_done: false,
        published_detecting: false,
        snapshot_tx,
        frames_tx,
    };
    tokio::spawn(engine.run(cmd_rx));
    handle
}

struct Engine {
    config: EngineConfig,
    scene: Scene,
    client: Option<Client>,
    /// Last connection failure worth telling the user about.
    problem: Option<String>,
    devices: Vec<Device>,
    extras: HashMap<u32, DeviceExtras>,
    /// Devices a sidecar is currently talking to; not streamed until released.
    held: HashSet<u32>,
    renderer: Renderer,
    /// Last frame sent per device; empty vec = nothing sent yet.
    last: Vec<Vec<Color>>,
    out: Vec<Vec<Color>>,
    paused: bool,
    blackout: Option<Blackout>,
    /// Send one frame even though streaming is paused (the lights came back on).
    force_frame: bool,
    /// Something changed since the last render; static scenes render once and
    /// then the tick returns immediately until this is set again.
    dirty: bool,
    fps_changed: bool,
    start: Instant,
    last_preview: Instant,
    /// Debounced device-list reload requested by the server.
    reload_at: Option<Instant>,
    /// When OpenRGB's current detection began (connect or a rescan).
    detect_since: Option<Instant>,
    /// OpenRGB said detection is complete.
    detection_done: bool,
    /// `detecting` as last published, to notice the grace period running out.
    published_detecting: bool,
    snapshot_tx: watch::Sender<Arc<Snapshot>>,
    frames_tx: broadcast::Sender<Arc<FrameSet>>,
}

impl Engine {
    fn make_ticker(fps: u32) -> tokio::time::Interval {
        let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / fps.max(1) as f64));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker
    }

    async fn run(mut self, mut cmd_rx: mpsc::Receiver<Command>) {
        let mut ticker = Self::make_ticker(self.config.fps);
        let mut events: Option<mpsc::UnboundedReceiver<Event>> = None;
        let mut next_connect = Instant::now();
        let mut backoff = Duration::from_secs(1);

        loop {
            if self.fps_changed {
                self.fps_changed = false;
                ticker = Self::make_ticker(self.config.fps);
            }
            if self.client.is_none() && Instant::now() >= next_connect {
                match self.connect().await {
                    Ok(rx) => {
                        events = Some(rx);
                        backoff = Duration::from_secs(1);
                        if self.problem.take().is_some() {
                            self.publish();
                        }
                    }
                    Err(e) => {
                        tracing::debug!(%e, "OpenRGB not reachable; retrying in {backoff:?}");
                        let problem = connection_problem(&e);
                        if problem != self.problem {
                            self.problem = problem;
                            self.publish();
                        }
                        next_connect = Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(10));
                    }
                }
            }

            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    None | Some(Command::Shutdown) => break,
                    Some(cmd) => self.handle(cmd).await,
                },
                ev = async {
                    match events.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let lost = match ev {
                        None => Some("event channel closed".to_string()),
                        Some(Event::Disconnected(reason)) => Some(reason),
                        Some(other) => {
                            self.on_event(other);
                            None
                        }
                    };
                    if let Some(reason) = lost {
                        tracing::warn!(%reason, "lost OpenRGB");
                        self.disconnect();
                        events = None;
                        next_connect = Instant::now() + Duration::from_secs(1);
                    }
                },
                _ = ticker.tick() => self.tick().await,
            }
        }
        tracing::info!("engine stopped");
    }

    async fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::SetScene(scene) => {
                self.scene = scene;
                self.scene.layout.ensure_placements(&self.devices);
                self.rebuild();
            }
            Command::SetLayers(layers) => {
                self.scene.layers = layers;
                self.dirty = true;
                self.publish();
            }
            Command::SetLayout(layout) => {
                self.scene.layout = layout;
                self.scene.layout.ensure_placements(&self.devices);
                self.rebuild();
            }
            Command::SetBrightness(b) => {
                self.scene.brightness = b.clamp(0.0, 1.0);
                self.dirty = true;
                self.publish();
            }
            Command::SetFps(fps) => {
                self.config.fps = fps.clamp(15, 120);
                self.fps_changed = true;
                self.publish();
            }
            Command::SetExtras(id, extras) => {
                if self.extras.get(&id) != Some(&extras) {
                    self.extras.insert(id, extras);
                    self.publish();
                }
            }
            Command::HoldDevice(id, hold) => {
                if hold {
                    self.held.insert(id);
                } else if self.held.remove(&id) {
                    // Resend a full frame once released.
                    if let Some(i) = self.devices.iter().position(|d| d.id == id) {
                        self.last[i].clear();
                    }
                    self.dirty = true;
                }
            }
            Command::Pause => {
                self.paused = true;
                self.publish();
            }
            Command::Resume => {
                self.paused = false;
                self.last.iter_mut().for_each(Vec::clear);
                self.dirty = true;
                self.publish();
            }
            Command::SetBlackout(blackout) => {
                if self.blackout == blackout {
                    return;
                }
                if self.blackout.is_some() && blackout.is_none() {
                    // Back on: resend every LED, even if streaming is paused.
                    self.last.iter_mut().for_each(Vec::clear);
                    self.force_frame = true;
                }
                self.blackout = blackout;
                self.dirty = true;
                // Let the preview show the change on the next tick.
                self.last_preview = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
                self.publish();
            }
            Command::RefreshDevices => {
                if let Err(e) = self.reload_devices().await {
                    tracing::warn!(%e, "device refresh failed");
                    self.disconnect();
                }
            }
            Command::Shutdown => {}
        }
    }

    fn on_event(&mut self, ev: Event) {
        match ev {
            Event::DeviceListUpdated => {
                self.reload_at = Some(Instant::now() + Duration::from_millis(500));
            }
            Event::DetectionStarted => {
                self.detect_since = Some(Instant::now());
                self.detection_done = false;
                self.publish();
            }
            Event::DetectionComplete => {
                self.detection_done = true;
                self.reload_at = Some(Instant::now() + Duration::from_millis(500));
                self.publish();
            }
            Event::ServerName(name) => tracing::info!(%name, "OpenRGB server"),
            Event::Log { level, message } => tracing::debug!(level, %message, "OpenRGB log"),
            other => tracing::trace!(?other, "OpenRGB event"),
        }
    }

    async fn connect(&mut self) -> Result<mpsc::UnboundedReceiver<Event>> {
        let (client, events) = Client::connect(self.config.addr, &self.config.client_name).await?;
        self.client = Some(client);
        self.detect_since = Some(Instant::now());
        self.detection_done = false;
        if let Err(e) = self.reload_devices().await {
            // Keep nothing of a half-initialised session: with the client still
            // set, the run loop would consider us connected and never try again.
            self.client = None;
            self.detect_since = None;
            return Err(e);
        }
        Ok(events)
    }

    fn disconnect(&mut self) {
        self.client = None;
        self.detect_since = None;
        self.devices.clear();
        self.rebuild();
    }

    /// Connected with no devices, and OpenRGB may still be finding them.
    fn detecting(&self) -> bool {
        self.client.is_some()
            && self.devices.is_empty()
            && !self.detection_done
            && self.detect_since.is_some_and(|t| t.elapsed() < DETECTION_GRACE)
    }

    async fn reload_devices(&mut self) -> Result<()> {
        let client = self.client.clone().ok_or(crate::Error::NotConnected)?;
        let mut devices = client.controllers().await?;
        devices.retain(|d| !d.leds.is_empty() && !d.is_hidden());
        for d in &devices {
            if d.direct_mode_index().is_some() {
                client.set_custom_mode(d.id).await?;
            }
        }
        tracing::info!(count = devices.len(), "devices loaded");
        self.extras.retain(|id, _| devices.iter().any(|d| d.id == *id));
        self.devices = devices;
        self.scene.layout.ensure_placements(&self.devices);
        self.rebuild();
        Ok(())
    }

    fn rebuild(&mut self) {
        self.renderer = Renderer::new(&self.devices, &self.scene.layout);
        self.last = vec![Vec::new(); self.devices.len()];
        self.out = vec![Vec::new(); self.devices.len()];
        self.dirty = true;
        self.publish();
    }

    fn publish(&mut self) {
        let detecting = self.detecting();
        self.published_detecting = detecting;
        let snapshot = Snapshot {
            connected: self.client.is_some(),
            problem: self.problem.clone(),
            paused: self.paused,
            blackout: self.blackout,
            detecting,
            fps: self.config.fps,
            devices: self.devices.iter().map(|d| DeviceInfo::from_device(d, self.extras.get(&d.id))).collect(),
            geometry: self.renderer.geometry().to_vec(),
            points: self.renderer.points().to_vec(),
            scene: self.scene.clone(),
        };
        self.snapshot_tx.send_replace(Arc::new(snapshot));
    }

    async fn tick(&mut self) {
        if self.reload_at.is_some_and(|at| Instant::now() >= at) {
            self.reload_at = None;
            if let Err(e) = self.reload_devices().await {
                tracing::warn!(%e, "device reload failed");
                self.disconnect();
                return;
            }
        }

        if self.published_detecting && !self.detecting() {
            // The grace period ran out with nothing found: tell the UI.
            self.publish();
        }
        let Some(client) = self.client.clone() else { return };
        if self.devices.is_empty() {
            return;
        }
        // Paused means "leave the devices as they are" — unless they have to go
        // dark, or come back from dark once.
        if self.paused && self.blackout.is_none() && !self.force_frame {
            return;
        }
        // A static scene (or a dark one) is sent once; afterwards the tick costs nothing.
        let animated = self.blackout.is_none() && !self.paused && self.scene.is_animated();
        if !self.dirty && !animated {
            return;
        }
        self.dirty = false;
        self.force_frame = false;

        let t = self.start.elapsed().as_secs_f64();
        if self.blackout.is_some() {
            self.out.resize_with(self.devices.len(), Vec::new);
            for (frame, device) in self.out.iter_mut().zip(&self.devices) {
                frame.clear();
                frame.resize(device.leds.len(), Color::BLACK);
            }
        } else {
            self.renderer.render(&self.scene, t, &mut self.out);
        }

        for (i, device) in self.devices.iter().enumerate() {
            if self.held.contains(&device.id) {
                continue;
            }
            if self.out[i] != self.last[i] {
                if let Err(e) = client.update_leds(device.id, &self.out[i]).await {
                    tracing::warn!(%e, device = %device.name, "frame send failed");
                    self.disconnect();
                    return;
                }
                self.last[i].clone_from(&self.out[i]);
            }
        }

        let preview_interval = Duration::from_secs_f64(1.0 / self.config.preview_fps.max(1) as f64);
        if self.frames_tx.receiver_count() > 0 && self.last_preview.elapsed() >= preview_interval {
            self.last_preview = Instant::now();
            let _ = self.frames_tx.send(Arc::new(FrameSet {
                t,
                device_ids: self.renderer.device_ids().to_vec(),
                frames: self.out.clone(),
            }));
        }
    }
}
