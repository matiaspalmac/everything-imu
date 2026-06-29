//! `PsMoveDevice` — implements the device-traits `Device` trait.

use crate::calibration::ImuCalibration;
use crate::hid::{spawn_reader, HidReaderHandle};
use crate::ids::ControllerKind;
use crate::report::{parse_report, ReportClock};
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceMetadata,
};
use hidapi::HidDevice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct PsMoveDevice {
    metadata: DeviceMetadata,
    kind: ControllerKind,
    calibration: ImuCalibration,
    device: Option<HidDevice>,
    io: Option<Arc<Mutex<HidDevice>>>,
    output_state: Arc<Mutex<OutputState>>,
    output_shutdown: Arc<AtomicBool>,
    output_join: Option<thread::JoinHandle<()>>,
    reader: Option<HidReaderHandle>,
}

#[derive(Debug, Clone, Copy)]
struct OutputState {
    rgb: [u8; 3],
    rumble: u8,
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
            calibration: ImuCalibration::identity(),
            device: Some(device),
            io: None,
            output_state: Arc::new(Mutex::new(OutputState {
                rgb: [0, 0, 0],
                rumble: 0,
            })),
            output_shutdown: Arc::new(AtomicBool::new(false)),
            output_join: None,
            reader: None,
        }
    }

    /// Install a factory calibration (read over USB via
    /// [`crate::pairing::read_factory_calibration`]) before `start`. Applied to
    /// every IMU sub-frame in the reader.
    pub fn set_calibration(&mut self, cal: ImuCalibration) {
        self.calibration = cal;
    }

    fn write_output_now(&self) -> Result<(), DeviceError> {
        let io = self
            .io
            .clone()
            .ok_or_else(|| DeviceError::Hid("psmove not started".into()))?;
        let state = *self
            .output_state
            .lock()
            .map_err(|_| DeviceError::Hid("psmove output lock poisoned".into()))?;
        write_output(&io, state)
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
        let dev = Arc::new(Mutex::new(dev));
        self.io = Some(dev.clone());
        let kind = self.kind;
        let device_id = self.metadata.id.clone();
        let connected_flag = Arc::new(AtomicBool::new(false));
        // Per-device sample clock: drives fusion dt from measured inter-report
        // timing (JC1 ratefix lesson), not the wrapping 16-bit hw counter.
        let mut clock = ReportClock::new();
        // Factory calibration (feature 0x10) is read over USB at pairing time and
        // persisted per-MAC; the BT IMU session starts from identity and VQF
        // warm-up covers residual bias until a stored blob is loaded here.
        let cal = self.calibration;
        let mut reader = spawn_reader(dev, move |buf, tx| {
            if !connected_flag.swap(true, Ordering::Relaxed) {
                let _ = tx.try_send(ChannelInfo::Connected(device_id.clone()));
            }
            if !parse_report(kind, buf, &mut clock, &cal, tx) {
                tracing::trace!(len = buf.len(), "psmove unknown report");
            }
        });
        let events_rx = std::mem::replace(&mut reader.events_rx, mpsc::channel(1).1);
        self.reader = Some(reader);
        self.output_shutdown.store(false, Ordering::Relaxed);
        let out_state = self.output_state.clone();
        let out_io = self
            .io
            .as_ref()
            .expect("io must be set before output thread")
            .clone();
        let out_stop = self.output_shutdown.clone();
        self.output_join = Some(
            thread::Builder::new()
                .name("device-psmove-output".into())
                .spawn(move || {
                    while !out_stop.load(Ordering::Relaxed) {
                        let state = match out_state.lock() {
                            Ok(s) => *s,
                            Err(_) => return,
                        };
                        if state.rgb != [0, 0, 0] || state.rumble != 0 {
                            let _ = write_output(&out_io, state);
                        }
                        // PS Move LED auto-turns-off after ~5s without a write,
                        // so 3s heartbeat keeps it lit while leaving margin.
                        thread::sleep(Duration::from_secs(3));
                    }
                })
                .map_err(|e| DeviceError::Hid(format!("psmove output thread failed: {e}")))?,
        );
        Ok(events_rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(mut r) = self.reader.take() {
            r.shutdown();
        }
        self.output_shutdown.store(true, Ordering::Relaxed);
        if let Some(j) = self.output_join.take() {
            let _ = j.join();
        }
        // Extinguish the sphere LED and stop rumble before dropping the handle;
        // otherwise the hardware holds the last colour/motor state until its own
        // ~5 s auto-off timer fires.
        if let Some(io) = self.io.as_ref() {
            let _ = write_output(
                io,
                OutputState {
                    rgb: [0, 0, 0],
                    rumble: 0,
                },
            );
        }
        self.io = None;
        Ok(())
    }

    async fn set_led_mask(&mut self, mask: u8) -> Result<(), DeviceError> {
        let mut state = self
            .output_state
            .lock()
            .map_err(|_| DeviceError::Hid("psmove output lock poisoned".into()))?;
        state.rgb = led_rgb_from_mask(mask);
        drop(state);
        self.write_output_now()
    }

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        let mut state = self
            .output_state
            .lock()
            .map_err(|_| DeviceError::Hid("psmove output lock poisoned".into()))?;
        state.rumble = device_traits::rumble::to_u8(intensity);
        drop(state);
        self.write_output_now()
    }
}

fn build_output_report(state: OutputState) -> [u8; 9] {
    [
        0x06, // report id
        0x00,
        state.rgb[0],
        state.rgb[1],
        state.rgb[2],
        0x00,
        state.rumble,
        0x00,
        0x00,
    ]
}

fn write_output(io: &Arc<Mutex<HidDevice>>, state: OutputState) -> Result<(), DeviceError> {
    let report = build_output_report(state);
    let dev = io
        .lock()
        .map_err(|_| DeviceError::Hid("psmove io lock poisoned".into()))?;
    dev.write(&report)
        .map_err(|e| DeviceError::Hid(format!("psmove write output failed: {e}")))?;
    Ok(())
}

fn led_rgb_from_mask(mask: u8) -> [u8; 3] {
    match (mask & 0x0F).count_ones() {
        0 => [0x00, 0x00, 0x00],
        1 => [0x00, 0x00, 0xFF],
        2 => [0x00, 0xFF, 0xFF],
        3 => [0xFF, 0x00, 0xFF],
        _ => [0xFF, 0xFF, 0xFF],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_report_layout_matches_protocol() {
        let report = build_output_report(OutputState {
            rgb: [0x12, 0x34, 0x56],
            rumble: 0x80,
        });
        assert_eq!(
            report,
            [0x06, 0x00, 0x12, 0x34, 0x56, 0x00, 0x80, 0x00, 0x00]
        );
    }

    #[test]
    fn led_mask_maps_to_expected_colors() {
        assert_eq!(led_rgb_from_mask(0b0000), [0x00, 0x00, 0x00]);
        assert_eq!(led_rgb_from_mask(0b0001), [0x00, 0x00, 0xFF]);
        assert_eq!(led_rgb_from_mask(0b0011), [0x00, 0xFF, 0xFF]);
        assert_eq!(led_rgb_from_mask(0b0111), [0xFF, 0x00, 0xFF]);
        assert_eq!(led_rgb_from_mask(0b1111), [0xFF, 0xFF, 0xFF]);
    }
}
