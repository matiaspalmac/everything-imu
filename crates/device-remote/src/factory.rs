use crate::device::{RemoteDevice, RemoteEvent};
use crate::protocol::{self, RemoteMsg};
use device_traits::{BatteryState, Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

const DEFAULT_BIND: &str = "0.0.0.0:9320";
const STALE_AFTER: Duration = Duration::from_secs(15);
const SWEEP_EVERY: Duration = Duration::from_secs(5);

struct Route {
    tx: mpsc::Sender<RemoteEvent>,
    last_seen: Instant,
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
                }
                recv = socket.recv_from(&mut buf) => {
                    let (n, peer) = recv.map_err(|e| {
                        DeviceError::Hid(format!("remote recv failed: {e}"))
                    })?;
                    let Some(msg) = protocol::parse(&buf[..n]) else { continue };
                    handle_msg(msg, peer, &socket, &mut routes, &out).await;
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
    out: &mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
) {
    match msg {
        RemoteMsg::Hello { name, .. } => {
            tracing::debug!(hub = %name, ip = %peer.ip(), "remote hub hello");
            let _ = socket.send_to(&protocol::encode_hello_ack(), peer).await;
        }
        RemoteMsg::Announce(a) => {
            let key = (peer.ip(), a.handle);
            if let Some(route) = routes.get_mut(&key) {
                route.last_seen = Instant::now();
                return;
            }
            let (tx, rx) = mpsc::channel::<RemoteEvent>(256);
            let dev = RemoteDevice::new(&a, peer, socket.clone(), rx);
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
                },
            );
        }
        RemoteMsg::Remove { handle } => {
            routes.remove(&(peer.ip(), handle));
        }
        RemoteMsg::Imu { handle, samples } => {
            route_event(routes, (peer.ip(), handle), RemoteEvent::Imu(samples)).await;
        }
        RemoteMsg::Battery {
            handle,
            fraction,
            charging,
        } => {
            route_event(
                routes,
                (peer.ip(), handle),
                RemoteEvent::Battery(BatteryState { fraction, charging }),
            )
            .await;
        }
        RemoteMsg::Button { handle, reset } => {
            route_event(routes, (peer.ip(), handle), RemoteEvent::Reset(reset)).await;
        }
    }
}

async fn route_event(
    routes: &mut HashMap<(IpAddr, u16), Route>,
    key: (IpAddr, u16),
    event: RemoteEvent,
) {
    if let Some(route) = routes.get_mut(&key) {
        route.last_seen = Instant::now();
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
}
