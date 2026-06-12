use crate::device::{RemoteDevice, RemoteEvent};
use crate::protocol::{self, RemoteMsg};
use device_traits::{BatteryState, Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, RwLock};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

const DEFAULT_BIND: &str = "0.0.0.0:9320";
const STALE_AFTER: Duration = Duration::from_secs(15);
const SWEEP_EVERY: Duration = Duration::from_secs(5);

struct Route {
    tx: mpsc::Sender<RemoteEvent>,
    last_seen: Instant,
    loss: LossStats,
}

impl Route {
    fn touch(&mut self) {
        self.last_seen = Instant::now();
    }
}

/// Per-route packet-loss tracking from MSG_IMU2 sequence numbers. Reports a
/// log line per 10 s window when anything was lost, then resets the window.
#[derive(Default)]
struct LossStats {
    last_seq: Option<u16>,
    received: u64,
    lost: u64,
    window_start: Option<Instant>,
}

impl LossStats {
    fn record(&mut self, seq: u16, key: &(IpAddr, u16)) {
        if let Some(prev) = self.last_seq {
            let gap = seq.wrapping_sub(prev);
            // gap 1 = in order; 0 or huge = duplicate/reorder/restart — skip.
            if gap > 1 && gap < 1000 {
                self.lost += u64::from(gap) - 1;
            }
        }
        self.last_seq = Some(seq);
        self.received += 1;
        let start = *self.window_start.get_or_insert_with(Instant::now);
        if start.elapsed() >= Duration::from_secs(10) {
            let total = self.received + self.lost;
            if total > 0 && self.lost > 0 {
                let pct = self.lost as f64 * 100.0 / total as f64;
                tracing::info!(
                    ip = %key.0, handle = key.1,
                    received = self.received, lost = self.lost,
                    loss_pct = format!("{pct:.1}"),
                    "remote packet loss (10s window)"
                );
            }
            self.received = 0;
            self.lost = 0;
            self.window_start = Some(Instant::now());
        }
    }
}

/// One rumble target per hub IP, shared by every RemoteDevice from that hub.
/// A per-route peer would only refresh on datagrams for its own handle, so a
/// handle that goes quiet (or a device outliving its route) keeps rumbling at
/// the hub's previous ephemeral source port after an app reconnect.
fn touch_hub_peer(
    hub_peers: &mut HashMap<IpAddr, Arc<RwLock<SocketAddr>>>,
    from: SocketAddr,
) -> Arc<RwLock<SocketAddr>> {
    let shared = hub_peers
        .entry(from.ip())
        .or_insert_with(|| Arc::new(RwLock::new(from)))
        .clone();
    if let Ok(mut p) = shared.write() {
        if *p != from {
            tracing::debug!(old = %*p, new = %from, "remote hub source port moved");
            *p = from;
        }
    }
    shared
}

/// eimu remote-hub UDP listener. One device per `(hub ip, handle)`.
#[derive(Clone)]
pub struct RemoteFactory {
    bind_addr: String,
}

impl RemoteFactory {
    pub fn new() -> Self {
        Self {
            bind_addr: DEFAULT_BIND.to_string(),
        }
    }

    pub fn with_bind_addr(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
        }
    }
}

impl Default for RemoteFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DeviceFactory for RemoteFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let socket = Arc::new(UdpSocket::bind(&self.bind_addr).await.map_err(|e| {
            DeviceError::Hid(format!("remote bind {} failed: {e}", self.bind_addr))
        })?);
        tracing::info!(addr = %self.bind_addr, "eimu remote-hub UDP listener online");

        let mut routes: HashMap<(IpAddr, u16), Route> = HashMap::new();
        let mut hub_peers: HashMap<IpAddr, Arc<RwLock<SocketAddr>>> = HashMap::new();
        let mut sweep = tokio::time::interval(SWEEP_EVERY);
        let mut buf = [0u8; 2048];
        loop {
            tokio::select! {
                _ = sweep.tick() => {
                    routes.retain(|key, route| {
                        let alive = route.last_seen.elapsed() < STALE_AFTER;
                        if !alive {
                            tracing::info!(ip = %key.0, handle = key.1, "remote device stale; dropping");
                        }
                        alive
                    });
                    hub_peers.retain(|ip, _| routes.keys().any(|(rip, _)| rip == ip));
                }
                recv = socket.recv_from(&mut buf) => {
                    let (n, peer) = recv.map_err(|e| {
                        DeviceError::Hid(format!("remote recv failed: {e}"))
                    })?;
                    let Some(msg) = protocol::parse(&buf[..n]) else { continue };
                    handle_msg(msg, peer, &socket, &mut routes, &mut hub_peers, &out).await;
                }
            }
        }
    }
}

async fn handle_msg(
    msg: RemoteMsg,
    peer: SocketAddr,
    socket: &Arc<UdpSocket>,
    routes: &mut HashMap<(IpAddr, u16), Route>,
    hub_peers: &mut HashMap<IpAddr, Arc<RwLock<SocketAddr>>>,
    out: &mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
) {
    match msg {
        RemoteMsg::Hello { name, .. } => {
            tracing::debug!(hub = %name, ip = %peer.ip(), "remote hub hello");
            // Hellos arrive every few seconds even when no handle is
            // streaming — refresh the rumble target only if the hub already
            // has devices (otherwise sweeps would never reap the entry).
            if routes.keys().any(|(rip, _)| *rip == peer.ip()) {
                touch_hub_peer(hub_peers, peer);
            }
            let _ = socket.send_to(&protocol::encode_hello_ack(), peer).await;
        }
        RemoteMsg::Announce(a) => {
            let shared_peer = touch_hub_peer(hub_peers, peer);
            let key = (peer.ip(), a.handle);
            if let Some(route) = routes.get_mut(&key) {
                route.touch();
                return;
            }
            let (tx, rx) = mpsc::channel::<RemoteEvent>(256);
            let dev = RemoteDevice::new(&a, shared_peer, socket.clone(), rx);
            let meta = dev.metadata().clone();
            tracing::info!(id = %meta.id, kind = ?meta.kind, "remote device announced");
            if out.send((meta, Box::new(dev))).await.is_err() {
                return;
            }
            routes.insert(
                key,
                Route {
                    tx,
                    last_seen: Instant::now(),
                    loss: LossStats::default(),
                },
            );
        }
        RemoteMsg::Remove { handle } => {
            routes.remove(&(peer.ip(), handle));
        }
        RemoteMsg::Imu {
            handle,
            seq,
            samples,
        } => {
            if let Some(seq) = seq {
                let key = (peer.ip(), handle);
                if let Some(route) = routes.get_mut(&key) {
                    route.loss.record(seq, &key);
                }
            }
            route_event(routes, hub_peers, peer, handle, RemoteEvent::Imu(samples)).await;
        }
        RemoteMsg::Battery {
            handle,
            fraction,
            charging,
        } => {
            route_event(
                routes,
                hub_peers,
                peer,
                handle,
                RemoteEvent::Battery(BatteryState { fraction, charging }),
            )
            .await;
        }
        RemoteMsg::Button { handle, reset } => {
            route_event(routes, hub_peers, peer, handle, RemoteEvent::Reset(reset)).await;
        }
    }
}

async fn route_event(
    routes: &mut HashMap<(IpAddr, u16), Route>,
    hub_peers: &mut HashMap<IpAddr, Arc<RwLock<SocketAddr>>>,
    peer: SocketAddr,
    handle: u16,
    event: RemoteEvent,
) {
    let key = (peer.ip(), handle);
    if let Some(route) = routes.get_mut(&key) {
        route.touch();
        touch_hub_peer(hub_peers, peer);
        if route.tx.send(event).await.is_err() {
            routes.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use device_traits::ChannelInfo;

    fn announce_pkt(handle: u16) -> Vec<u8> {
        let mut b = vec![0x45, 0x49, 0x4D, 0x55, 0x01, protocol::MSG_ANNOUNCE];
        b.extend_from_slice(&handle.to_le_bytes());
        b.push(protocol::KIND_PHONE);
        b.extend_from_slice(&[2, 0, 0, 0, 0, 9]);
        b.extend_from_slice(&[1, 1, 1]);
        b.extend_from_slice(&200u16.to_le_bytes());
        b.push(2);
        b.extend_from_slice(b"Px");
        b
    }

    fn imu_pkt(handle: u16) -> Vec<u8> {
        let mut b = vec![0x45, 0x49, 0x4D, 0x55, 0x01, protocol::MSG_IMU];
        b.extend_from_slice(&handle.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&42u64.to_le_bytes());
        for v in [0.1f32, 0.2, 0.3, 0.0, 0.0, 9.8] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        b.extend_from_slice(&[0u8; 12]);
        b
    }

    #[tokio::test]
    async fn hello_gets_ack_and_announce_registers_device() {
        let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = probe.local_addr().unwrap();
        drop(probe);
        let factory = RemoteFactory::with_bind_addr(server_addr.to_string());
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(8);
        let server = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut hello = vec![0x45, 0x49, 0x4D, 0x55, 0x01, protocol::MSG_HELLO];
        hello.extend_from_slice(&[7u8; 16]);
        hello.push(2);
        hello.extend_from_slice(b"Px");
        client.send_to(&hello, server_addr).await.unwrap();
        let mut buf = [0u8; 32];
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buf))
            .await
            .expect("ack timeout")
            .unwrap();
        assert_eq!(&buf[..n], &protocol::encode_hello_ack()[..]);

        client.send_to(&announce_pkt(0), server_addr).await.unwrap();
        let (meta, mut dev) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("registration timeout")
            .expect("device");
        assert_eq!(meta.kind, device_traits::DeviceKind::Phone);
        assert_eq!(meta.capabilities.native_imu_rate_hz, 200);

        let mut ch = dev.start().await.unwrap();
        assert!(matches!(
            ch.recv().await.unwrap(),
            ChannelInfo::Connected(_)
        ));
        client.send_to(&imu_pkt(0), server_addr).await.unwrap();
        let evt = tokio::time::timeout(Duration::from_secs(2), ch.recv())
            .await
            .expect("imu timeout")
            .unwrap();
        let ChannelInfo::ImuSamples(samples) = evt else {
            panic!("expected samples, got {evt:?}");
        };
        assert_eq!(samples[0].timestamp_us, 42);

        // Re-announce must NOT register a second device.
        client.send_to(&announce_pkt(0), server_addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(rx.try_recv().is_err());

        server.abort();
    }

    fn announce_pkt_kind(handle: u16, kind: u8) -> Vec<u8> {
        let mut b = announce_pkt(handle);
        b[6 + 2] = kind; // kind byte sits after header + handle
        b
    }

    fn imu_pkt_ts(handle: u16, ts: u64) -> Vec<u8> {
        let mut b = imu_pkt(handle);
        b[6 + 3..6 + 3 + 8].copy_from_slice(&ts.to_le_bytes());
        b
    }

    #[tokio::test]
    async fn one_hub_streams_phone_and_controllers_concurrently() {
        let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = probe.local_addr().unwrap();
        drop(probe);
        let factory = RemoteFactory::with_bind_addr(server_addr.to_string());
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(8);
        let server = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        // Same hub announces phone (0), a BLE Joy-Con 2 (1), and an
        // InputDevice-forwarded DualSense (1000).
        for (handle, kind) in [
            (0u16, protocol::KIND_PHONE),
            (1u16, protocol::KIND_JOYCON2_L),
            (1000u16, protocol::KIND_DUALSENSE),
        ] {
            client
                .send_to(&announce_pkt_kind(handle, kind), server_addr)
                .await
                .unwrap();
        }

        let mut channels = Vec::new();
        for _ in 0..3 {
            let (meta, mut dev) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("registration timeout")
                .expect("device");
            let mut ch = dev.start().await.unwrap();
            assert!(matches!(
                ch.recv().await.unwrap(),
                ChannelInfo::Connected(_)
            ));
            channels.push((meta.kind, ch));
        }
        let kinds: Vec<_> = channels.iter().map(|(k, _)| *k).collect();
        assert!(kinds.contains(&device_traits::DeviceKind::Phone));
        assert!(kinds.contains(&device_traits::DeviceKind::JoyCon2L));
        assert!(kinds.contains(&device_traits::DeviceKind::DualSense));

        // Interleaved IMU traffic routes to the right device by handle.
        for (handle, ts) in [
            (0u16, 10u64),
            (1u16, 20u64),
            (1000u16, 30u64),
            (0u16, 11u64),
        ] {
            client
                .send_to(&imu_pkt_ts(handle, ts), server_addr)
                .await
                .unwrap();
        }
        let expect_ts = |kind: device_traits::DeviceKind| match kind {
            device_traits::DeviceKind::Phone => vec![10u64, 11],
            device_traits::DeviceKind::JoyCon2L => vec![20],
            _ => vec![30],
        };
        for (kind, ch) in channels.iter_mut() {
            for want in expect_ts(*kind) {
                let evt = tokio::time::timeout(Duration::from_secs(2), ch.recv())
                    .await
                    .expect("imu timeout")
                    .unwrap();
                let ChannelInfo::ImuSamples(samples) = evt else {
                    panic!("expected samples for {kind:?}, got {evt:?}");
                };
                assert_eq!(samples[0].timestamp_us, want, "wrong routing for {kind:?}");
            }
        }

        server.abort();
    }

    fn imu2_pkt(handle: u16, seq: u16, ts: u64) -> Vec<u8> {
        let mut b = vec![0x45, 0x49, 0x4D, 0x55, 0x01, protocol::MSG_IMU2];
        b.extend_from_slice(&handle.to_le_bytes());
        b.extend_from_slice(&seq.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&ts.to_le_bytes());
        for v in [0.1f32, 0.2, 0.3, 0.0, 0.0, 9.8] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        b.extend_from_slice(&[0u8; 12]);
        b
    }

    #[tokio::test]
    async fn imu2_routes_samples_like_imu() {
        let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = probe.local_addr().unwrap();
        drop(probe);
        let factory = RemoteFactory::with_bind_addr(server_addr.to_string());
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(8);
        let server = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(&announce_pkt(0), server_addr).await.unwrap();
        let (_meta, mut dev) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("registration timeout")
            .expect("device");
        let mut ch = dev.start().await.unwrap();
        assert!(matches!(
            ch.recv().await.unwrap(),
            ChannelInfo::Connected(_)
        ));

        // A seq gap (1 -> 5) must not disturb routing — loss is only counted.
        for (seq, ts) in [(0u16, 10u64), (1, 11), (5, 12)] {
            client
                .send_to(&imu2_pkt(0, seq, ts), server_addr)
                .await
                .unwrap();
        }
        for want in [10u64, 11, 12] {
            let evt = tokio::time::timeout(Duration::from_secs(2), ch.recv())
                .await
                .expect("imu timeout")
                .unwrap();
            let ChannelInfo::ImuSamples(samples) = evt else {
                panic!("expected samples, got {evt:?}");
            };
            assert_eq!(samples[0].timestamp_us, want);
        }

        server.abort();
    }

    #[tokio::test]
    async fn rumble_follows_hub_across_socket_reconnect() {
        let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = probe.local_addr().unwrap();
        drop(probe);
        let factory = RemoteFactory::with_bind_addr(server_addr.to_string());
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(8);
        let server = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Hub announces a phone and a gamepad from socket A.
        let socket_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        for (handle, kind) in [
            (0u16, protocol::KIND_PHONE),
            (1000u16, protocol::KIND_DUALSENSE),
        ] {
            socket_a
                .send_to(&announce_pkt_kind(handle, kind), server_addr)
                .await
                .unwrap();
        }
        let mut pad_dev = None;
        for _ in 0..2 {
            let (meta, dev) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("registration timeout")
                .expect("device");
            if meta.kind == device_traits::DeviceKind::DualSense {
                pad_dev = Some(dev);
            }
        }
        let mut pad_dev = pad_dev.expect("gamepad registered");

        // App reconnects: socket B takes over, but only the phone keeps
        // announcing. Rumble for the gamepad must still reach socket B.
        let socket_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket_b
            .send_to(&announce_pkt_kind(0, protocol::KIND_PHONE), server_addr)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        pad_dev.set_rumble(1.0).await.unwrap();
        let mut buf = [0u8; 32];
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), socket_b.recv_from(&mut buf))
            .await
            .expect("rumble did not follow the hub to its new socket")
            .unwrap();
        assert_eq!(buf[5], protocol::MSG_RUMBLE);
        assert_eq!(n, 12);

        server.abort();
    }
}
