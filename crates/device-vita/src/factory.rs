use crate::device::{metadata_for_key, VitaDevice, VitaPacket};
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashMap;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

const DEFAULT_BIND: &str = "0.0.0.0:9306";

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

        let mut routing: HashMap<String, mpsc::Sender<VitaPacket>> = HashMap::new();
        let mut buf = [0u8; 64];
        loop {
            let (n, peer) = socket
                .recv_from(&mut buf)
                .await
                .map_err(|e| DeviceError::Hid(format!("vita recv failed: {e}")))?;
            let Some(packet) = VitaPacket::parse(&buf[..n]) else {
                continue;
            };
            let key = peer.ip().to_string();

            if let Some(tx) = routing.get(&key) {
                if tx.send(packet).await.is_err() {
                    routing.remove(&key);
                }
                continue;
            }
            let (pkt_tx, pkt_rx) = mpsc::channel::<VitaPacket>(256);
            let meta = metadata_for_key(&key);
            let dev = VitaDevice::new(meta.clone(), pkt_rx);
            if out.send((meta, Box::new(dev))).await.is_err() {
                return Ok(());
            }
            let _ = pkt_tx.send(packet).await;
            routing.insert(key, pkt_tx);
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
