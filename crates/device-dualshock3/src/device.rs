//! DualShock 3 device: hidapi transport, enable handshake, motion reader.

use crate::report::{imu_from_motion, parse_input_report};
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind, DeviceMetadata,
};
use hidapi::{HidApi, HidDevice};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;
use tokio::sync::mpsc;

static HID_API: OnceLock<Arc<Mutex<HidApi>>> = OnceLock::new();

pub(crate) fn hid_api_singleton() -> Result<Arc<Mutex<HidApi>>, hidapi::HidError> {
    if let Some(api) = HID_API.get() {
        return Ok(api.clone());
    }
    let api = HidApi::new()?;
    let _ = HID_API.set(Arc::new(Mutex::new(api)));
    Ok(HID_API.get().unwrap().clone())
}

/// Feature-report handshake that makes the DS3 begin streaming input reports.
/// USB form: SET_FEATURE report `0xF4` = `42 0C 00 00`.
const ENABLE_FEATURE: [u8; 5] = [0xF4, 0x42, 0x0C, 0x00, 0x00];

pub struct DualShock3Device {
    metadata: DeviceMetadata,
    device: Option<HidDevice>,
    shutdown: Arc<AtomicBool>,
    reader: Option<thread::JoinHandle<()>>,
}

impl DualShock3Device {
    pub fn new(device: HidDevice, serial: String, mac: [u8; 6]) -> Self {
        let metadata = DeviceMetadata {
            id: DeviceId { mac, serial },
            kind: DeviceKind::DualShock3,
            firmware: None,
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: false,
                has_rumble: false,
                // 3-axis accel + single yaw gyro; ~100 Hz typical.
                native_imu_rate_hz: 100,
            },
        };
        Self {
            metadata,
            device: Some(device),
            shutdown: Arc::new(AtomicBool::new(false)),
            reader: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for DualShock3Device {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let device = self
            .device
            .take()
            .ok_or_else(|| DeviceError::Hid("ds3 already started".into()))?;
        // Kick the pad into streaming. Some stacks reject this silently; the
        // reader still works if the OS already enabled reports.
        if let Err(e) = device.send_feature_report(&ENABLE_FEATURE) {
            tracing::warn!(error = %e, "ds3 enable feature report rejected; continuing");
        }

        let (tx, rx) = mpsc::channel::<ChannelInfo>(64);
        let id = self.metadata.id.clone();
        let shutdown = self.shutdown.clone();
        let reader = thread::Builder::new()
            .name("device-dualshock3-hid".into())
            .spawn(move || {
                let _ = tx.blocking_send(ChannelInfo::Connected(id));
                let _ = device.set_blocking_mode(true);
                let start = Instant::now();
                let mut buf = [0u8; 64];
                while !shutdown.load(Ordering::Relaxed) {
                    match device.read_timeout(&mut buf, 50) {
                        Ok(0) => continue,
                        Ok(n) => {
                            if let Some(m) = parse_input_report(&buf[..n]) {
                                let imu = imu_from_motion(m, start, Instant::now());
                                if tx
                                    .blocking_send(ChannelInfo::ImuSamples(vec![imu]))
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "ds3 hid read error → gone");
                            let _ = tx.blocking_send(ChannelInfo::Disconnected);
                            return;
                        }
                    }
                }
                let _ = tx.blocking_send(ChannelInfo::Disconnected);
            })
            .map_err(|e| DeviceError::Hid(format!("ds3 reader spawn failed: {e}")))?;
        self.reader = Some(reader);
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        self.shutdown.store(true, Ordering::Relaxed);
        // Reader exits on its next 50 ms read boundary; detach rather than block.
        self.reader.take();
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_rumble(&mut self, _intensity: f32) -> Result<(), DeviceError> {
        Ok(())
    }
}

impl Drop for DualShock3Device {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
