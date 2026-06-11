use crate::protocol::{encode_rumble, Announce};
use device_traits::{
    BatteryState, ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceMetadata,
    ImuSample, ResetKind,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Routed events for one remote handle, produced by the factory's recv loop.
#[derive(Debug)]
pub enum RemoteEvent {
    Imu(Vec<ImuSample>),
    Battery(BatteryState),
    Reset(ResetKind),
}

pub struct RemoteDevice {
    metadata: DeviceMetadata,
    event_rx: Option<mpsc::Receiver<RemoteEvent>>,
    reader: Option<JoinHandle<()>>,
    /// Shared server socket + the hub's address, for the rumble backchannel.
    socket: Arc<UdpSocket>,
    peer: SocketAddr,
    handle: u16,
    /// Announced with `rate_hz = 0`: a haptics-only endpoint (phone in
    /// gamepads-only hub role). The device is registered so haptics rules can
    /// target it, but it never emits pipeline events — no `Connected`, so no
    /// `sensor_info` reaches SlimeVR and no ghost tracker appears.
    haptics_only: bool,
}

impl RemoteDevice {
    pub fn new(
        announce: &Announce,
        peer: SocketAddr,
        socket: Arc<UdpSocket>,
        event_rx: mpsc::Receiver<RemoteEvent>,
    ) -> Self {
        Self {
            metadata: DeviceMetadata {
                id: DeviceId {
                    mac: announce.mac,
                    serial: format!("remote-{}-{}", peer.ip(), announce.handle),
                },
                kind: announce.kind,
                firmware: Some(format!("remote:{}", announce.name)),
                capabilities: DeviceCapabilities {
                    has_magnetometer: announce.has_mag,
                    has_battery: announce.has_battery,
                    has_rumble: announce.has_rumble,
                    native_imu_rate_hz: announce.rate_hz.max(1),
                },
            },
            event_rx: Some(event_rx),
            reader: None,
            socket,
            peer,
            handle: announce.handle,
            haptics_only: announce.rate_hz == 0,
        }
    }
}

#[async_trait::async_trait]
impl Device for RemoteDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let mut event_rx = self
            .event_rx
            .take()
            .ok_or_else(|| DeviceError::Hid("remote device already started".into()))?;
        let (tx, rx) = mpsc::channel::<ChannelInfo>(256);
        let id = self.metadata.id.clone();
        let haptics_only = self.haptics_only;
        self.reader = Some(tokio::spawn(async move {
            if !haptics_only {
                let _ = tx.send(ChannelInfo::Connected(id)).await;
            }
            while let Some(ev) = event_rx.recv().await {
                if haptics_only {
                    // Swallow stray events — this endpoint must stay invisible
                    // to the SlimeVR pipeline.
                    continue;
                }
                let out = match ev {
                    RemoteEvent::Imu(samples) => ChannelInfo::ImuSamples(samples),
                    RemoteEvent::Battery(b) => ChannelInfo::Battery(b),
                    RemoteEvent::Reset(r) => ChannelInfo::ResetRequested(r),
                };
                if tx.send(out).await.is_err() {
                    break;
                }
            }
            // The factory dropped the sender: REMOVE message or stale timeout.
            if !haptics_only {
                let _ = tx.send(ChannelInfo::Disconnected).await;
            }
        }));
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(h) = self.reader.take() {
            h.abort();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        let pkt = encode_rumble(self.handle, intensity.clamp(0.0, 1.0));
        self.socket
            .send_to(&pkt, self.peer)
            .await
            .map_err(|e| DeviceError::Hid(format!("remote rumble send failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::KIND_PHONE;

    fn announce() -> Announce {
        Announce {
            handle: 0,
            kind: device_traits::DeviceKind::Phone,
            mac: [2, 0, 0, 0, 0, 1],
            has_mag: true,
            has_battery: true,
            has_rumble: true,
            rate_hz: 200,
            name: "Pixel".into(),
        }
    }

    #[tokio::test]
    async fn haptics_only_endpoint_emits_no_pipeline_events_but_rumbles() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer = receiver.local_addr().unwrap();
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let (tx, rx) = mpsc::channel(8);
        let mut a = announce();
        a.rate_hz = 0; // haptics-only marker
        let mut dev = RemoteDevice::new(&a, peer, sock, rx);
        let mut ch = dev.start().await.unwrap();

        // No Connected, and stray events are swallowed.
        tx.send(RemoteEvent::Battery(BatteryState {
            fraction: 0.5,
            charging: false,
        }))
        .await
        .unwrap();
        drop(tx); // route gone — must NOT surface Disconnected either
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(300), ch.recv())
                .await
                .map(|e| e.is_none())
                .unwrap_or(true),
            "haptics-only device leaked a pipeline event"
        );

        // Rumble backchannel still works.
        dev.set_rumble(1.0).await.unwrap();
        let mut buf = [0u8; 32];
        let (n, _) = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            receiver.recv_from(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(n, 12);
        assert_eq!(buf[5], crate::protocol::MSG_RUMBLE);
    }

    #[tokio::test]
    async fn maps_events_to_channel_info_and_disconnects_on_close() {
        let _ = KIND_PHONE; // silence unused import lint in this mod
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let peer: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let (tx, rx) = mpsc::channel(8);
        let mut dev = RemoteDevice::new(&announce(), peer, sock, rx);
        assert_eq!(dev.metadata().kind, device_traits::DeviceKind::Phone);
        let mut ch = dev.start().await.unwrap();
        assert!(matches!(
            ch.recv().await.unwrap(),
            ChannelInfo::Connected(_)
        ));
        tx.send(RemoteEvent::Imu(vec![ImuSample {
            gyro: [0.0; 3],
            accel: [0.0, 0.0, 9.8],
            mag: None,
            timestamp_us: 1,
        }]))
        .await
        .unwrap();
        assert!(matches!(
            ch.recv().await.unwrap(),
            ChannelInfo::ImuSamples(v) if v.len() == 1
        ));
        tx.send(RemoteEvent::Reset(ResetKind::Yaw)).await.unwrap();
        assert!(matches!(
            ch.recv().await.unwrap(),
            ChannelInfo::ResetRequested(ResetKind::Yaw)
        ));
        drop(tx);
        assert!(matches!(
            ch.recv().await.unwrap(),
            ChannelInfo::Disconnected
        ));
    }

    #[tokio::test]
    async fn rumble_sends_datagram_to_peer() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer = receiver.local_addr().unwrap();
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let (_tx, rx) = mpsc::channel(1);
        let mut dev = RemoteDevice::new(&announce(), peer, sock, rx);
        dev.set_rumble(0.5).await.unwrap();
        let mut buf = [0u8; 32];
        let (n, _) = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            receiver.recv_from(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(buf[5], crate::protocol::MSG_RUMBLE);
        assert_eq!(n, 12);
    }
}
