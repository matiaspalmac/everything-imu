//! `DualSenseDevice` — implements the device-traits `Device` trait.

use crate::hid::{spawn_reader, HidReaderHandle};
use crate::ids::ControllerKind;
use crate::report::{parse_feature_calibration, parse_ps_button, parse_report, SonyCalibration};
use device_traits::{
    ButtonState, ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceMetadata,
    ResetButtonDetector,
};
use hidapi::HidDevice;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;

pub struct DualSenseDevice {
    metadata: DeviceMetadata,
    kind: ControllerKind,
    device: Option<HidDevice>,
    io: Option<Arc<Mutex<HidDevice>>>,
    calibration: Option<SonyCalibration>,
    output_state: OutputState,
    last_report_len: Arc<AtomicUsize>,
    reader: Option<HidReaderHandle>,
}

#[derive(Debug, Clone, Copy)]
struct OutputState {
    led_mask_4bit: u8,
    led_mask_5bit: u8,
    /// Motor amplitude 0-255. DualSense/DS4 drive both motors at this value.
    rumble: u8,
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
            io: None,
            calibration: None,
            output_state: OutputState {
                led_mask_4bit: 0,
                led_mask_5bit: 0,
                rumble: 0,
            },
            last_report_len: Arc::new(AtomicUsize::new(0)),
            reader: None,
        }
    }

    fn write_output(&self, state: OutputState) -> Result<(), DeviceError> {
        let io = self
            .io
            .clone()
            .ok_or_else(|| DeviceError::Hid("dualsense not started".into()))?;
        let kind = self.kind;
        let last_len = self.last_report_len.load(Ordering::Relaxed);
        if last_len == 0 {
            // No input report observed yet → transport unknown. Skip rather than
            // guess and ship a USB-shaped report to a BT-attached pad (or vice
            // versa). Caller will retry on next set_led/set_rumble.
            tracing::trace!("dualsense set_output before first read — deferred");
            return Ok(());
        }
        let is_bt = last_len >= 78;
        let report = build_output_report(kind, state, is_bt);
        if report.is_empty() {
            return Ok(());
        }
        let dev = io
            .lock()
            .map_err(|_| DeviceError::Hid("dualsense io lock poisoned".into()))?;
        dev.write(&report)
            .map_err(|e| DeviceError::Hid(format!("dualsense write output failed: {e}")))?;
        Ok(())
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
        self.calibration = read_feature_calibration(&dev, self.kind);
        let dev = Arc::new(Mutex::new(dev));
        self.io = Some(dev.clone());
        let kind = self.kind;
        let calibration = self.calibration;
        let report_len = self.last_report_len.clone();
        let mut reset_detector = ResetButtonDetector::new();
        let device_id = self.metadata.id.clone();
        let connected_flag = Arc::new(AtomicBool::new(false));
        let mut reader = spawn_reader(dev, move |buf, tx| {
            if !connected_flag.swap(true, Ordering::Relaxed) {
                let _ = tx.try_send(ChannelInfo::Connected(device_id.clone()));
            }
            report_len.store(buf.len(), Ordering::Relaxed);
            if !parse_report(kind, buf, calibration, tx) {
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
        self.io = None;
        Ok(())
    }

    async fn set_led_mask(&mut self, mask: u8) -> Result<(), DeviceError> {
        self.output_state.led_mask_4bit = mask & 0x0F;
        self.output_state.led_mask_5bit = map_led_mask_to_dualsense(mask);
        self.write_output(self.output_state)
    }

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        self.output_state.rumble = device_traits::rumble::to_u8(intensity);
        self.write_output(self.output_state)
    }
}

fn read_feature_calibration(device: &HidDevice, kind: ControllerKind) -> Option<SonyCalibration> {
    let mut buf_05 = [0u8; 41];
    buf_05[0] = 0x05;
    match device.get_feature_report(&mut buf_05) {
        Ok(_) => match parse_feature_calibration(kind, 0x05, &buf_05) {
            Some(cal) => {
                tracing::info!(
                    ?kind,
                    report = "0x05",
                    bias_dps = ?cal.gyro_bias_dps,
                    gyro_scale = ?cal.gyro_scale,
                    accel_bias_g = ?cal.accel_bias_g,
                    accel_scale = ?cal.accel_scale,
                    "DualSense/DS4 factory calibration applied"
                );
                return Some(cal);
            }
            None => tracing::warn!(
                ?kind,
                report = "0x05",
                "feature report read but parse_feature_calibration rejected payload"
            ),
        },
        Err(e) => tracing::warn!(?kind, report = "0x05", error = %e, "get_feature_report failed"),
    }
    if matches!(kind, ControllerKind::DualShock4) {
        let mut buf_02 = [0u8; 37];
        buf_02[0] = 0x02;
        match device.get_feature_report(&mut buf_02) {
            Ok(_) => match parse_feature_calibration(kind, 0x02, &buf_02) {
                Some(cal) => {
                    tracing::info!(
                        ?kind,
                        report = "0x02",
                        bias_dps = ?cal.gyro_bias_dps,
                        "DS4 USB factory calibration applied"
                    );
                    return Some(cal);
                }
                None => tracing::warn!(
                    ?kind,
                    report = "0x02",
                    "DS4 USB cal payload rejected by parser"
                ),
            },
            Err(e) => {
                tracing::warn!(?kind, report = "0x02", error = %e, "DS4 USB get_feature_report failed")
            }
        }
    }
    tracing::warn!(
        ?kind,
        "no factory calibration loaded; gyro bias relies on VQF rest-bias estimator (capped ~2 deg/s)"
    );
    None
}

fn map_led_mask_to_dualsense(mask: u8) -> u8 {
    match (mask & 0x0F).count_ones() {
        0 => 0b00000,
        1 => 0b00100,
        2 => 0b01010,
        3 => 0b10101,
        _ => 0b11111,
    }
}

fn build_output_report(kind: ControllerKind, state: OutputState, bt: bool) -> Vec<u8> {
    match kind {
        ControllerKind::DualSense | ControllerKind::DualSenseEdge => {
            if bt {
                build_ds5_bt_report(state)
            } else {
                build_ds5_usb_report(state)
            }
        }
        ControllerKind::DualShock4 => {
            if bt {
                build_ds4_bt_report(state)
            } else {
                build_ds4_usb_report(state)
            }
        }
    }
}

fn build_ds5_usb_report(state: OutputState) -> Vec<u8> {
    let mut report = vec![0u8; 48];
    report[0] = 0x02;
    fill_ds5_payload(&mut report[1..], state);
    report
}

fn build_ds5_bt_report(state: OutputState) -> Vec<u8> {
    let mut report = vec![0u8; 78];
    report[0] = 0x31;
    report[1] = 0x02;
    fill_ds5_payload(&mut report[2..], state);
    let crc = crc32_with_seed(0xA2, &report[..74]);
    report[74] = (crc & 0xFF) as u8;
    report[75] = ((crc >> 8) & 0xFF) as u8;
    report[76] = ((crc >> 16) & 0xFF) as u8;
    report[77] = ((crc >> 24) & 0xFF) as u8;
    report
}

fn fill_ds5_payload(payload: &mut [u8], state: OutputState) {
    payload[0] = 0xFF;
    payload[1] = 0xF7;
    let motor = state.rumble;
    payload[2] = motor; // weak
    payload[3] = motor; // strong
    payload[38] = 0;
    payload[39] = 1;
    payload[40] = state.led_mask_5bit & 0x1F;
}

fn build_ds4_usb_report(state: OutputState) -> Vec<u8> {
    let mut report = vec![0u8; 32];
    report[0] = 0x05;
    report[1] = 0x07;
    let motor = state.rumble;
    report[4] = motor; // weak
    report[5] = motor; // strong
    let [r, g, b] = ds4_led_rgb_from_mask(state.led_mask_4bit);
    report[6] = r;
    report[7] = g;
    report[8] = b;
    report
}

fn build_ds4_bt_report(state: OutputState) -> Vec<u8> {
    let mut report = vec![0u8; 78];
    report[0] = 0x11;
    report[1] = 0xC0;
    report[2] = 0x20;
    report[3] = 0x07;
    let motor = state.rumble;
    report[6] = motor; // weak
    report[7] = motor; // strong
    let [r, g, b] = ds4_led_rgb_from_mask(state.led_mask_4bit);
    report[8] = r;
    report[9] = g;
    report[10] = b;
    let crc = crc32_with_seed(0xA2, &report[..74]);
    report[74] = (crc & 0xFF) as u8;
    report[75] = ((crc >> 8) & 0xFF) as u8;
    report[76] = ((crc >> 16) & 0xFF) as u8;
    report[77] = ((crc >> 24) & 0xFF) as u8;
    report
}

fn ds4_led_rgb_from_mask(mask: u8) -> [u8; 3] {
    match (mask & 0x0F).count_ones() {
        0 => [0x00, 0x00, 0x00],
        1 => [0x00, 0x00, 0xFF],
        2 => [0x00, 0xFF, 0xFF],
        3 => [0xFF, 0x00, 0xFF],
        _ => [0xFF, 0xFF, 0xFF],
    }
}

fn crc32_with_seed(seed: u8, payload: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    crc = crc32_update(crc, seed);
    for b in payload {
        crc = crc32_update(crc, *b);
    }
    !crc
}

fn crc32_update(mut crc: u32, b: u8) -> u32 {
    crc ^= b as u32;
    for _ in 0..8 {
        let mask = (crc & 1).wrapping_neg() & 0xEDB8_8320;
        crc = (crc >> 1) ^ mask;
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_mask_maps_to_5_led_patterns() {
        assert_eq!(map_led_mask_to_dualsense(0b0000), 0b00000);
        assert_eq!(map_led_mask_to_dualsense(0b0001), 0b00100);
        assert_eq!(map_led_mask_to_dualsense(0b0011), 0b01010);
        assert_eq!(map_led_mask_to_dualsense(0b0111), 0b10101);
        assert_eq!(map_led_mask_to_dualsense(0b1111), 0b11111);
    }

    #[test]
    fn ds5_bt_report_has_crc_and_payload() {
        let report = build_ds5_bt_report(OutputState {
            led_mask_4bit: 0,
            led_mask_5bit: 0b00100,
            rumble: 0x80,
        });
        assert_eq!(report.len(), 78);
        assert_eq!(report[0], 0x31);
        assert_eq!(report[1], 0x02);
        assert_eq!(report[4], 0x80);
        assert_eq!(report[5], 0x80);
        assert_eq!(report[42], 0b00100);
        let crc = crc32_with_seed(0xA2, &report[..74]);
        assert_eq!(&report[74..78], &crc.to_le_bytes());
    }

    #[test]
    fn ds4_usb_report_has_led_and_rumble() {
        let report = build_ds4_usb_report(OutputState {
            led_mask_4bit: 0b0011,
            led_mask_5bit: 0,
            rumble: 0x80,
        });
        assert_eq!(report.len(), 32);
        assert_eq!(report[0], 0x05);
        assert_eq!(report[1], 0x07);
        assert_eq!(report[4], 0x80);
        assert_eq!(report[5], 0x80);
        assert_eq!(&report[6..9], &[0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn ds4_bt_report_has_crc_and_led() {
        let report = build_ds4_bt_report(OutputState {
            led_mask_4bit: 0b0111,
            led_mask_5bit: 0,
            rumble: 0,
        });
        assert_eq!(report.len(), 78);
        assert_eq!(report[0], 0x11);
        assert_eq!(report[1], 0xC0);
        assert_eq!(report[2], 0x20);
        assert_eq!(report[3], 0x07);
        assert_eq!(&report[8..11], &[0xFF, 0x00, 0xFF]);
        let crc = crc32_with_seed(0xA2, &report[..74]);
        assert_eq!(&report[74..78], &crc.to_le_bytes());
    }
}
