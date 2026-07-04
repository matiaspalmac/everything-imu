use crate::device::{metadata_for_key, VitaDevice, VitaPacket};
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

const DEFAULT_BIND: &str = "0.0.0.0:9306";

/// Evict a routing entry after this long without packets, so a Vita that never
/// reconnects does not leak a slot forever. Dropping the sender makes the
/// device's reader see `recv()` return `None` and emit `Disconnected`.
const ROUTE_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

/// Listens for Vita homebrew IMU packets over UDP and registers one device per
/// sender IP (one Vita = one tracker). Same shape as the 3DS forwarder, on its
/// own port and with the 24-byte float payload.
#[derive(Clone)]
pub struct VitaFactory {
    bind_addr: String,
}

impl VitaFactory {
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

impl Default for VitaFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DeviceFactory for VitaFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let socket = UdpSocket::bind(&self.bind_addr)
            .await
            .map_err(|e| DeviceError::Hid(format!("vita bind {} failed: {e}", self.bind_addr)))?;
        tracing::info!(addr = %self.bind_addr, "vita forwarded UDP listener online");

        let mut routing: HashMap<String, (mpsc::Sender<VitaPacket>, Instant)> = HashMap::new();
        let mut buf = [0u8; 64];
        loop {
            let (n, peer) = match socket.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    // Transient errors must not tear down the listener.
                    tracing::warn!("vita recv failed: {e}");
                    continue;
                }
            };
            let now = Instant::now();
            // Reap routes idle past the timeout; dropping the sender signals stop.
            routing.retain(|_, (_, last)| now.duration_since(*last) < ROUTE_IDLE_TIMEOUT);

            let Some(packet) = VitaPacket::parse(&buf[..n]) else {
                continue;
            };
            let key = peer.ip().to_string();

            if let Some((tx, last)) = routing.get_mut(&key) {
                *last = now;
                // Drop-oldest, non-blocking: a slow/dead consumer must not stall
                // the shared listener for the other trackers.
                match tx.try_send(packet) {
                    Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        routing.remove(&key);
                    }
                }
                continue;
            }
            let (pkt_tx, pkt_rx) = mpsc::channel::<VitaPacket>(256);
            let meta = metadata_for_key(&key);
            let dev = VitaDevice::new(meta.clone(), pkt_rx);
            if out.send((meta, Box::new(dev))).await.is_err() {
                return Ok(());
            }
            let _ = pkt_tx.try_send(packet);
            routing.insert(key, (pkt_tx, now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registers_device_on_first_packet() {
        let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = probe.local_addr().unwrap();
        drop(probe);

        let factory = VitaFactory::with_bind_addr(server_addr.to_string());
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(8);
        let handle = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut raw = [0u8; 24];
        raw[8..12].copy_from_slice(&1.0f32.to_le_bytes()); // az = 1 g
        client.send_to(&raw, server_addr).await.unwrap();

        let (meta, mut dev) = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("device registered")
            .expect("some device");
        assert_eq!(meta.kind, device_traits::DeviceKind::Vita);

        let mut ch = dev.start().await.unwrap();
        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), ch.recv())
            .await
            .expect("connected event")
            .expect("some event");
        assert!(matches!(evt, device_traits::ChannelInfo::Connected(_)));

        handle.abort();
    }
}
