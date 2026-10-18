//! Tokio TCP client. One connection, one reader task; requests are answered in
//! order by the server, so pending responses are matched FIFO by packet ID.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::error::{Error, Result};
use crate::model::{Color, Device, Mode};
use crate::parse;
use crate::protocol::{ack_status, client_flags, packet, Header, DEFAULT_PORT, HEADER_LEN, PROTOCOL_VERSION};
use crate::wire::{cstr, cstr_to_string, Reader};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

/// Unsolicited packets from the server.
#[derive(Debug, Clone)]
pub enum Event {
    DeviceListUpdated,
    DetectionStarted,
    DetectionProgress { percent: u32, message: String },
    DetectionComplete,
    ServerName(String),
    ServerFlags(u32),
    Ack { pkt_id: u32, status: u32 },
    ActiveProfileChanged(String),
    ProfileListUpdated,
    /// The server waits for an ACK from ProfileManager clients before loading.
    ProfileAboutToLoad,
    ProfileLoaded(String),
    Log { level: u32, message: String },
    Unknown { dev_id: u32, pkt_id: u32, payload: Vec<u8> },
    /// The reader task ended; every later call fails with [`Error::Disconnected`].
    Disconnected(String),
}

struct Pending {
    seq: u64,
    dev_id: u32,
    pkt_id: u32,
    tx: oneshot::Sender<Result<Vec<u8>>>,
}

struct Inner {
    writer: Mutex<OwnedWriteHalf>,
    pending: StdMutex<VecDeque<Pending>>,
    next_seq: AtomicU64,
    version: AtomicU32,
    events: mpsc::UnboundedSender<Event>,
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl Client {
    /// Connect to `127.0.0.1:6742`.
    pub async fn connect_local(client_name: &str) -> Result<(Client, mpsc::UnboundedReceiver<Event>)> {
        Self::connect(SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)), client_name).await
    }

    /// Connect, negotiate the protocol version and announce `client_name`.
    pub async fn connect(addr: SocketAddr, client_name: &str) -> Result<(Client, mpsc::UnboundedReceiver<Event>)> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let (rd, wr) = stream.into_split();
        let (events_tx, events_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(Inner {
            writer: Mutex::new(wr),
            pending: StdMutex::new(VecDeque::new()),
            next_seq: AtomicU64::new(0),
            version: AtomicU32::new(0),
            events: events_tx,
        });
        tokio::spawn(read_loop(rd, Arc::clone(&inner)));

        let client = Client { inner };
        let version = client.negotiate_version().await?;
        client.inner.version.store(version, Ordering::Relaxed);
        tracing::info!(version, %addr, "connected to OpenRGB");

        client.send(0, packet::SET_CLIENT_NAME, &cstr(client_name)).await?;
        if version >= 6 {
            let flags = client_flags::RGB_CONTROLLER | client_flags::PROFILE_MANAGER | client_flags::REQUEST_LOCAL;
            client.send(0, packet::SET_CLIENT_FLAGS, &flags.to_le_bytes()).await?;
        }
        Ok((client, events_rx))
    }

    /// Negotiated protocol version (0 for servers that never answered).
    pub fn protocol_version(&self) -> u32 {
        self.inner.version.load(Ordering::Relaxed)
    }

    async fn negotiate_version(&self) -> Result<u32> {
        let ours = PROTOCOL_VERSION.to_le_bytes();
        match self.request(0, packet::REQUEST_PROTOCOL_VERSION, &ours, VERSION_TIMEOUT).await {
            Ok(payload) => Ok(Reader::new(&payload).u32()?.min(PROTOCOL_VERSION)),
            Err(Error::Timeout) => Ok(0),
            Err(e) => Err(e),
        }
    }

    fn require(&self, needed: u32) -> Result<()> {
        let actual = self.protocol_version();
        if actual < needed {
            return Err(Error::Unsupported { needed, actual });
        }
        Ok(())
    }

    // ---- controllers -------------------------------------------------------

    /// Controller IDs in server order (v6) or `0..count` on older servers.
    pub async fn controller_ids(&self) -> Result<Vec<u32>> {
        let payload = self.request(0, packet::REQUEST_CONTROLLER_COUNT, &[], DEFAULT_TIMEOUT).await?;
        parse::parse_controller_ids(&payload, self.protocol_version())
    }

    pub async fn controller(&self, id: u32) -> Result<Device> {
        let version = self.protocol_version();
        let req: &[u8] = if version >= 1 { &version.to_le_bytes() } else { &[] };
        let payload = self.request(id, packet::REQUEST_CONTROLLER_DATA, req, DEFAULT_TIMEOUT).await?;
        parse::parse_device(&payload, id, version)
    }

    pub async fn controllers(&self) -> Result<Vec<Device>> {
        let ids = self.controller_ids().await?;
        let mut devices = Vec::with_capacity(ids.len());
        for id in ids {
            devices.push(self.controller(id).await?);
        }
        Ok(devices)
    }

    /// Switch the controller to its per-LED ("Direct"/custom) mode.
    pub async fn set_custom_mode(&self, id: u32) -> Result<()> {
        self.send(id, packet::RGBCONTROLLER_SETCUSTOMMODE, &[]).await
    }

    /// Push a full frame: one colour per LED, in LED order.
    pub async fn update_leds(&self, id: u32, colors: &[Color]) -> Result<()> {
        self.send(id, packet::RGBCONTROLLER_UPDATELEDS, &parse::encode_update_leds(colors)?).await
    }

    /// Push full frames for several controllers in a single write, so the
    /// server takes them in together, followed by a request whose answer
    /// (see [`Receipt`]) shows it has read them all. A streamer that waits for
    /// that before sending more never piles frames up behind a slow server.
    pub async fn update_leds_batch(&self, frames: &[(u32, &[Color])]) -> Result<Receipt> {
        let mut wire = Vec::new();
        for (id, colors) in frames {
            append_frame(&mut wire, *id, packet::RGBCONTROLLER_UPDATELEDS, &parse::encode_update_leds(colors)?);
        }
        // Any request would do; this one is cheap and changes nothing.
        append_frame(&mut wire, 0, packet::REQUEST_CONTROLLER_COUNT, &[]);

        let (tx, rx) = oneshot::channel();
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed);
        let mut writer = self.inner.writer.lock().await;
        self.inner.pending.lock().unwrap().push_back(Pending { seq, dev_id: 0, pkt_id: packet::REQUEST_CONTROLLER_COUNT, tx });
        if let Err(e) = writer.write_all(&wire).await {
            self.inner.remove_pending(seq);
            return Err(e.into());
        }
        Ok(Receipt(rx))
    }

    pub async fn update_zone_leds(&self, id: u32, zone_idx: u32, colors: &[Color]) -> Result<()> {
        let payload = parse::encode_update_zone_leds(zone_idx, colors)?;
        self.send(id, packet::RGBCONTROLLER_UPDATEZONELEDS, &payload).await
    }

    pub async fn update_single_led(&self, id: u32, led_idx: i32, color: Color) -> Result<()> {
        let payload = parse::encode_update_single_led(led_idx, color);
        self.send(id, packet::RGBCONTROLLER_UPDATESINGLELED, &payload).await
    }

    /// Apply `mode` (as edited) as mode `mode_idx` on the controller.
    pub async fn update_mode(&self, id: u32, mode_idx: i32, mode: &Mode) -> Result<()> {
        let payload = parse::encode_update_mode(mode_idx, mode, self.protocol_version());
        self.send(id, packet::RGBCONTROLLER_UPDATEMODE, &payload).await
    }

    /// Persist `mode` to the device's flash. Only meaningful when the mode has
    /// [`crate::ModeFlags::MANUAL_SAVE`]; requires protocol v3.
    pub async fn save_mode(&self, id: u32, mode_idx: i32, mode: &Mode) -> Result<()> {
        self.require(3)?;
        let payload = parse::encode_update_mode(mode_idx, mode, self.protocol_version());
        self.send(id, packet::RGBCONTROLLER_SAVEMODE, &payload).await
    }

    pub async fn update_zone_mode(&self, id: u32, zone_idx: u32, mode_idx: i32, mode: &Mode) -> Result<()> {
        self.require(6)?;
        let payload = parse::encode_update_zone_mode(zone_idx, mode_idx, mode, self.protocol_version());
        self.send(id, packet::RGBCONTROLLER_UPDATEZONEMODE, &payload).await
    }

    pub async fn resize_zone(&self, id: u32, zone_idx: i32, new_size: i32) -> Result<()> {
        let payload = parse::encode_resize_zone(zone_idx, new_size);
        self.send(id, packet::RGBCONTROLLER_RESIZEZONE, &payload).await
    }

    /// Ask the server to re-run device detection (clients hear `DeviceListUpdated`).
    pub async fn rescan_devices(&self) -> Result<()> {
        self.require(6)?;
        self.send(0, packet::REQUEST_RESCAN_DEVICES, &[]).await
    }

    // ---- profiles ----------------------------------------------------------

    pub async fn profile_list(&self) -> Result<Vec<String>> {
        self.require(2)?;
        let payload = self.request(0, packet::PROFILEMANAGER_GET_PROFILE_LIST, &[], DEFAULT_TIMEOUT).await?;
        parse::parse_profile_list(&payload)
    }

    pub async fn load_profile(&self, name: &str) -> Result<()> {
        self.require(2)?;
        self.send(0, packet::PROFILEMANAGER_LOAD_PROFILE, &cstr(name)).await
    }

    pub async fn save_profile(&self, name: &str) -> Result<()> {
        self.require(2)?;
        self.send(0, packet::PROFILEMANAGER_SAVE_PROFILE, &cstr(name)).await
    }

    pub async fn delete_profile(&self, name: &str) -> Result<()> {
        self.require(2)?;
        self.send(0, packet::PROFILEMANAGER_DELETE_PROFILE, &cstr(name)).await
    }

    pub async fn active_profile(&self) -> Result<String> {
        self.require(6)?;
        let payload = self.request(0, packet::PROFILEMANAGER_GET_ACTIVE_PROFILE, &[], DEFAULT_TIMEOUT).await?;
        Ok(cstr_to_string(&payload))
    }

    // ---- settings ----------------------------------------------------------

    /// Settings JSON for `key` (e.g. `"Server"`).
    pub async fn settings(&self, key: &str) -> Result<String> {
        self.require(6)?;
        let payload = self.request(0, packet::SETTINGSMANAGER_GET_SETTINGS, &cstr(key), DEFAULT_TIMEOUT).await?;
        Ok(cstr_to_string(&payload))
    }

    // ---- transport ---------------------------------------------------------

    /// Send a packet without waiting for a reply.
    pub async fn send(&self, dev_id: u32, pkt_id: u32, payload: &[u8]) -> Result<()> {
        let mut writer = self.inner.writer.lock().await;
        write_frame(&mut writer, dev_id, pkt_id, payload).await
    }

    /// Send a packet and wait for the matching reply. The pending entry is
    /// registered while holding the writer lock so pending order equals wire order.
    pub async fn request(&self, dev_id: u32, pkt_id: u32, payload: &[u8], timeout: Duration) -> Result<Vec<u8>> {
        let (tx, rx) = oneshot::channel();
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed);

        let sent = {
            let mut writer = self.inner.writer.lock().await;
            self.inner.pending.lock().unwrap().push_back(Pending { seq, dev_id, pkt_id, tx });
            write_frame(&mut writer, dev_id, pkt_id, payload).await
        };
        if let Err(e) = sent {
            self.inner.remove_pending(seq);
            return Err(e);
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::Disconnected),
            Err(_) => {
                self.inner.remove_pending(seq);
                Err(Error::Timeout)
            }
        }
    }
}

/// Answer to the request [`Client::update_leds_batch`] sends after its frames.
/// The server handles a connection's packets in order, so once it arrives the
/// server has read every frame written before it.
pub struct Receipt(oneshot::Receiver<Result<Vec<u8>>>);

impl Receipt {
    /// `None` while the server has not got that far; `Some(Ok(()))` once it
    /// has; `Some(Err(_))` when the connection ended first.
    pub fn check(&mut self) -> Option<Result<()>> {
        match self.0.try_recv() {
            Ok(result) => Some(result.map(drop)),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(Error::Disconnected)),
        }
    }

    /// Wait until the server has read the frames.
    pub async fn wait(self) -> Result<()> {
        self.0.await.map_err(|_| Error::Disconnected)?.map(drop)
    }
}

fn append_frame(out: &mut Vec<u8>, dev_id: u32, pkt_id: u32, payload: &[u8]) {
    let header = Header { dev_id, pkt_id, size: payload.len() as u32 };
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(payload);
}

async fn write_frame(writer: &mut OwnedWriteHalf, dev_id: u32, pkt_id: u32, payload: &[u8]) -> Result<()> {
    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    append_frame(&mut frame, dev_id, pkt_id, payload);
    writer.write_all(&frame).await?;
    Ok(())
}

async fn read_loop(mut rd: OwnedReadHalf, inner: Arc<Inner>) {
    let outcome: Result<()> = async {
        let mut hdr = [0u8; HEADER_LEN];
        loop {
            rd.read_exact(&mut hdr).await?;
            let header = Header::decode(&hdr)?;
            let mut payload = vec![0u8; header.size as usize];
            rd.read_exact(&mut payload).await?;
            inner.dispatch(header, payload);
        }
    }
    .await;

    let reason = match outcome {
        Ok(()) => "connection closed".to_string(),
        Err(e) => e.to_string(),
    };
    tracing::warn!(%reason, "OpenRGB connection ended");
    for p in inner.pending.lock().unwrap().drain(..) {
        let _ = p.tx.send(Err(Error::Disconnected));
    }
    let _ = inner.events.send(Event::Disconnected(reason));
}

impl Inner {
    fn remove_pending(&self, seq: u64) {
        self.pending.lock().unwrap().retain(|p| p.seq != seq);
    }

    fn dispatch(&self, header: Header, payload: Vec<u8>) {
        {
            let mut pending = self.pending.lock().unwrap();
            if let Some(front) = pending.front() {
                if front.pkt_id == header.pkt_id && front.dev_id == header.dev_id {
                    let p = pending.pop_front().expect("front exists");
                    let _ = p.tx.send(Ok(payload));
                    return;
                }
            }
            // A failed ACK for the request at the front of the queue means no
            // data reply is coming; fail it now instead of waiting for the timeout.
            if header.pkt_id == packet::ACK {
                let mut r = Reader::new(&payload);
                let (acked, status) = (r.u32().unwrap_or(0), r.u32().unwrap_or(0));
                if status != ack_status::OK {
                    if let Some(front) = pending.front() {
                        if front.pkt_id == acked {
                            let p = pending.pop_front().expect("front exists");
                            let _ = p.tx.send(Err(Error::Rejected { pkt_id: acked, status }));
                            return;
                        }
                    }
                }
            }
        }

        let event = match header.pkt_id {
            packet::DEVICE_LIST_UPDATED => Event::DeviceListUpdated,
            packet::DETECTION_STARTED => Event::DetectionStarted,
            packet::DETECTION_PROGRESS_CHANGED => {
                let mut r = Reader::new(&payload);
                let _size = r.u32();
                Event::DetectionProgress {
                    percent: r.u32().unwrap_or(0),
                    message: r.str16().unwrap_or_default(),
                }
            }
            packet::DETECTION_COMPLETE => Event::DetectionComplete,
            packet::SET_SERVER_NAME => Event::ServerName(cstr_to_string(&payload)),
            packet::SET_SERVER_FLAGS => Event::ServerFlags(Reader::new(&payload).u32().unwrap_or(0)),
            packet::ACK => {
                let mut r = Reader::new(&payload);
                Event::Ack { pkt_id: r.u32().unwrap_or(0), status: r.u32().unwrap_or(0) }
            }
            packet::PROFILEMANAGER_ACTIVE_PROFILE_CHANGED => Event::ActiveProfileChanged(cstr_to_string(&payload)),
            packet::PROFILEMANAGER_PROFILE_LIST_UPDATED => Event::ProfileListUpdated,
            packet::PROFILEMANAGER_PROFILE_ABOUT_TO_LOAD => Event::ProfileAboutToLoad,
            packet::PROFILEMANAGER_PROFILE_LOADED => Event::ProfileLoaded(cstr_to_string(&payload)),
            packet::LOGMANAGER_LOGGED_ENTRY => {
                let mut r = Reader::new(&payload);
                let _size = r.u32();
                let level = r.u32().unwrap_or(0);
                let _line = r.u32();
                let _timestamp = r.u32();
                let _file = r.str16();
                Event::Log { level, message: r.str16().unwrap_or_default() }
            }
            pkt_id => Event::Unknown { dev_id: header.dev_id, pkt_id, payload },
        };
        let _ = self.events.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A tiny fake server: answers the version request, then serves one controller.
    async fn fake_server(listener: TcpListener, server_version: u32, device_payload: Vec<u8>) {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut hdr = [0u8; HEADER_LEN];
        loop {
            if sock.read_exact(&mut hdr).await.is_err() {
                return;
            }
            let h = Header::decode(&hdr).unwrap();
            let mut payload = vec![0u8; h.size as usize];
            sock.read_exact(&mut payload).await.unwrap();
            let reply = match h.pkt_id {
                packet::REQUEST_PROTOCOL_VERSION => Some(server_version.to_le_bytes().to_vec()),
                packet::REQUEST_CONTROLLER_COUNT => {
                    let mut v = 1u32.to_le_bytes().to_vec();
                    v.extend_from_slice(&0xC0FFEEu32.to_le_bytes());
                    Some(v)
                }
                packet::REQUEST_CONTROLLER_DATA => Some(device_payload.clone()),
                packet::RGBCONTROLLER_UPDATELEDS => {
                    // Unsolicited notification interleaved with traffic.
                    let n = Header { dev_id: 0, pkt_id: packet::DEVICE_LIST_UPDATED, size: 0 };
                    sock.write_all(&n.encode()).await.unwrap();
                    None
                }
                _ => None,
            };
            if let Some(body) = reply {
                let rh = Header { dev_id: h.dev_id, pkt_id: h.pkt_id, size: body.len() as u32 };
                sock.write_all(&rh.encode()).await.unwrap();
                sock.write_all(&body).await.unwrap();
            }
        }
    }

    fn minimal_device_payload() -> Vec<u8> {
        use crate::wire::Writer;
        let mut w = Writer::new();
        w.u32(0);
        w.i32(7).str16("Mat").str16("Razer").str16("").str16("1.0").str16("SN").str16("loc");
        w.u16(0).i32(0); // no modes
        w.u16(0); // no zones
        w.u16(1).str16("LED 0"); // one LED (v6: no value)
        w.u16(1).u32(Color::new(1, 2, 3).to_u32());
        w.u16(0).u32(0); // v5: display names, flags
        w.str16("Mat").str32("{}"); // v6
        let len = w.len() as u32;
        w.patch_u32(0, len);
        w.finish()
    }

    #[tokio::test]
    async fn handshake_enumerate_and_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(fake_server(listener, 6, minimal_device_payload()));

        let (client, mut events) = Client::connect(addr, "test").await.unwrap();
        assert_eq!(client.protocol_version(), 6);

        let devices = client.controllers().await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, 0xC0FFEE);
        assert_eq!(devices[0].name, "Mat");
        assert_eq!(devices[0].colors, vec![Color::new(1, 2, 3)]);

        client.update_leds(0xC0FFEE, &[Color::WHITE]).await.unwrap();
        let ev = tokio::time::timeout(Duration::from_secs(2), events.recv()).await.unwrap().unwrap();
        assert!(matches!(ev, Event::DeviceListUpdated));
    }

    #[tokio::test]
    async fn batch_receipt_comes_after_the_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(fake_server(listener, 6, minimal_device_payload()));

        let (client, mut events) = Client::connect(addr, "test").await.unwrap();
        let mut receipt = client.update_leds_batch(&[(1, &[Color::WHITE][..]), (2, &[Color::BLACK, Color::WHITE][..])]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while receipt.check().is_none() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        // The fake server announces every frame it reads, before it answers the request.
        for _ in 0..2 {
            assert!(matches!(events.try_recv(), Ok(Event::DeviceListUpdated)));
        }
        client.update_leds_batch(&[]).await.unwrap().wait().await.unwrap();
    }

    #[tokio::test]
    async fn negotiates_down_to_older_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(fake_server(listener, 4, Vec::new()));

        let (client, _events) = Client::connect(addr, "test").await.unwrap();
        assert_eq!(client.protocol_version(), 4);
        assert!(matches!(client.rescan_devices().await, Err(Error::Unsupported { needed: 6, actual: 4 })));
    }
}
