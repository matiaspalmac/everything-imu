//! `PsMoveDevice` — implements the device-traits `Device` trait.

use crate::hid::{spawn_reader, HidReaderHandle};
use crate::ids::ControllerKind;
use crate::report::parse_report;
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceMetadata,
};
use hidapi::HidDevice;
use tokio::sync::mpsc;

pub struct PsMoveDevice {
    metadata: DeviceMetadata,
    kind: ControllerKind,
    device: Option<HidDevice>,
    reader: Option<HidReaderHandle>,
}

impl PsMoveDevice {
    pub fn new(device: HidDevice, kind: ControllerKind, serial: String, mac: [u8; 6]) -> Self {
        let metadata = DeviceMetadata {
            id: DeviceId { mac, serial },
            kind: kind.into_device_kind(),
            firmware: None,
            capabilities: DeviceCapabilities {
                has_magnetometer: kind.has_magnetometer(),
                has_battery: true,
                has_rumble: true,
                native_imu_rate_hz: 175,
            },
        };
        Self {
            metadata,
            kind,
            device: Some(device),
            reader: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for PsMoveDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let dev = self
            .device
            .take()
            .ok_or_else(|| DeviceError::Hid("device handle already taken".into()))?;
        let kind = self.kind;
        let mut reader = spawn_reader(dev, move |buf, tx| {
            if !parse_report(kind, buf, tx) {
                tracing::trace!(len = buf.len(), "psmove unknown report");
            }
        });
        let events_rx = std::mem::replace(&mut reader.events_rx, mpsc::channel(1).1);
        self.reader = Some(reader);
        Ok(events_rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(mut r) = self.reader.take() {
            r.shutdown();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        // Output reports (sphere LED, rumble) intentionally out of scope —
        // the bridge only forwards motion to SlimeVR-Server.
        Ok(())
    }

    async fn set_rumble(&mut self, _on: bool) -> Result<(), DeviceError> {
        Ok(())
    }
}
