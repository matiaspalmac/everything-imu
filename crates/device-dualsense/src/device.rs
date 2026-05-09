//! `DualSenseDevice` — implements the device-traits `Device` trait.

use crate::hid::{spawn_reader, HidReaderHandle};
use crate::ids::ControllerKind;
use crate::report::{parse_ps_button, parse_report};
use device_traits::{
    ButtonState, ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceMetadata,
    ResetButtonDetector,
};
use hidapi::HidDevice;
use std::time::Instant;
use tokio::sync::mpsc;

pub struct DualSenseDevice {
    metadata: DeviceMetadata,
    kind: ControllerKind,
    device: Option<HidDevice>,
    reader: Option<HidReaderHandle>,
}

impl DualSenseDevice {
    pub fn new(device: HidDevice, kind: ControllerKind, serial: String, mac: [u8; 6]) -> Self {
        let metadata = DeviceMetadata {
            id: DeviceId { mac, serial },
            kind: kind.into_device_kind(),
            firmware: None,
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: true,
                has_rumble: true,
                native_imu_rate_hz: match kind {
                    ControllerKind::DualSense | ControllerKind::DualSenseEdge => 250,
                    ControllerKind::DualShock4 => 250,
                },
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
impl Device for DualSenseDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let dev = self
            .device
            .take()
            .ok_or_else(|| DeviceError::Hid("device handle already taken".into()))?;
        let kind = self.kind;
        let mut reset_detector = ResetButtonDetector::new();
        let mut reader = spawn_reader(dev, move |buf, tx| {
            if !parse_report(kind, buf, tx) {
                tracing::trace!(len = buf.len(), "dualsense unknown report");
                return;
            }
            if let Some(ps) = parse_ps_button(kind, buf) {
                let bs = ButtonState::HomeOrCapture {
                    home_pressed: ps,
                    capture_pressed: false,
                };
                if let Some(reset) = reset_detector.observe(bs, Instant::now()) {
                    let _ = tx.try_send(ChannelInfo::ResetRequested(reset));
                }
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
        // Output reports (LED / haptics / triggers) are intentionally out of scope —
        // the bridge only forwards IMU motion to SlimeVR-Server.
        Ok(())
    }

    async fn set_rumble(&mut self, _on: bool) -> Result<(), DeviceError> {
        Ok(())
    }
}
