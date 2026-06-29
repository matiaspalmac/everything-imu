use crate::device::{metadata_for_key, ThreeDsDevice, ThreeDsPacket};
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

const DEFAULT_BIND: &str = "0.0.0.0:9305";

/// Drop a per-console route after this much silence so abandoned or spoofed
/// source IPs cannot grow the routing map without bound. Active consoles send
/// at ~100 Hz, well inside this window.
const ROUTE_IDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// A live route to a registered device, plus the last time it saw a packet.
struct RouteEntry {
    tx: mpsc::Sender<ThreeDsPacket>,
    last_seen: Instant,
}

/// Listens for 3DS homebrew IMU packets over UDP and registers one device per
/// sender IP. Stateless on the wire — packets carry no id, so the source IP is
/// the tracker identity (one console = one tracker).
#[derive(Clone)]
pub struct ThreeDsFactory {
    bind_addr: String,
}

impl ThreeDsFactory {
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

impl Default for ThreeDsFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DeviceFactory for ThreeDsFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let socket = UdpSocket::bind(&self.bind_addr)
            .await
            .map_err(|e| DeviceError::Hid(format!("3ds bind {} failed: {e}", self.bind_addr)))?;
        tracing::info!(addr = %self.bind_addr, "3ds forwarded UDP listener online");

        let mut routing: HashMap<String, RouteEntry> = HashMap::new();
        let mut buf = [0u8; 64];
        loop {
            let (n, peer) = socket
                .recv_from(&mut buf)
                .await
                .map_err(|e| DeviceError::Hid(format!("3ds recv failed: {e}")))?;
            let Some(packet) = ThreeDsPacket::parse(&buf[..n]) else {
                // Wrong size — ignore stray traffic on the port.
                continue;
            };
            let key = peer.ip().to_string();
            let now = Instant::now();

            // Evict routes that have gone silent. Dropping the sender makes the
            // idle device observe a closed channel and emit Disconnected.
            routing.retain(|_, entry| now.duration_since(entry.last_seen) < ROUTE_IDLE_TIMEOUT);

            // Reuse a live route, or register a fresh device for a new console.
            // Use try_send so a stalled device cannot block the shared listener:
            // a full channel drops the sample, a closed channel drops the route.
            if let Some(entry) = routing.get_mut(&key) {
                entry.last_seen = now;
                match entry.tx.try_send(packet) {
                    Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        routing.remove(&key);
                    }
                }
                continue;
            }
            let (pkt_tx, pkt_rx) = mpsc::channel::<ThreeDsPacket>(256);
            let meta = metadata_for_key(&key);
            let dev = ThreeDsDevice::new(meta.clone(), pkt_rx);
            if out.send((meta, Box::new(dev))).await.is_err() {
                return Ok(());
            }
            let _ = pkt_tx.try_send(packet);
            routing.insert(
                key,
                RouteEntry {
                    tx: pkt_tx,
                    last_seen: now,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registers_device_on_first_packet_and_routes_by_ip() {
        // Discover a free port via the OS, then bind the factory to it.
        let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = probe.local_addr().unwrap();
        drop(probe);

        let factory = ThreeDsFactory::with_bind_addr(server_addr.to_string());
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(8);
        let handle = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });

        // Give the listener a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let raw = [
            0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00,
        ];
        client.send_to(&raw, server_addr).await.unwrap();

        let (meta, mut dev) = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("device registered")
            .expect("some device");
        assert_eq!(meta.kind, device_traits::DeviceKind::ThreeDs);

        let mut ch = dev.start().await.unwrap();
        // First event is Connected.
        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), ch.recv())
            .await
            .expect("connected event")
            .expect("some event");
        assert!(matches!(evt, device_traits::ChannelInfo::Connected(_)));

        handle.abort();
    }
}
