//! `SteamControllerDevice` — `Device` trait impl for the wired + dongle
//! Steam Controller. BLE transport is deferred (custom segmentation).

use crate::ids::SteamControllerTransport;
use crate::report::{parse_state, STATE_BODY_LEN};
use crate::scale::{accel_m_s2, gyro_rad_s};
use crate::subcmd::{
    build_simple, enable_imu_raw, ID_CLEAR_DIGITAL_MAPPINGS, ID_LOAD_DEFAULT_SETTINGS,
};
use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind, DeviceMetadata,
    ImuSample,
};
use hidapi::HidDevice;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Instant;
use tokio::sync::mpsc;

pub struct SteamControllerDevice {
    metadata: DeviceMetadata,
    transport: SteamControllerTransport,
    hid: Arc<StdMutex<HidDevice>>,
    shutdown: Option<tokio::sync::watch::Sender<bool>>,
    join: Option<std::thread::JoinHandle<()>>,
    epoch: Instant,
}

impl SteamControllerDevice {
    pub fn new(
        device: HidDevice,
        transport: SteamControllerTransport,
        serial: String,
        mac: [u8; 6],
    ) -> Self {
        let id = DeviceId { mac, serial };
        let metadata = DeviceMetadata {
            id,
            kind: DeviceKind::SteamController,
            firmware: Some(transport.label().into()),
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: matches!(transport, SteamControllerTransport::UsbDongle),
                // Haptic feature report is not implemented yet; advertise the
                // capability only once set_rumble actually drives the hardware.
                has_rumble: false,
                native_imu_rate_hz: 100,
            },
        };
        Self {
            metadata,
            transport,
            hid: Arc::new(StdMutex::new(device)),
            shutdown: None,
            join: None,
            epoch: Instant::now(),
        }
    }

    pub fn transport(&self) -> SteamControllerTransport {
        self.transport
    }

    fn run_init_sequence(hid: &Arc<StdMutex<HidDevice>>) -> Result<(), DeviceError> {
        let guard = hid.lock().unwrap();
        for buf in [
            build_simple(ID_CLEAR_DIGITAL_MAPPINGS),
            build_simple(ID_LOAD_DEFAULT_SETTINGS),
            enable_imu_raw(),
        ] {
            guard
                .send_feature_report(&buf)
                .map_err(|e| DeviceError::Hid(e.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Device for SteamControllerDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        if self.shutdown.is_some() {
            return Err(DeviceError::Hid("already started".into()));
        }

        Self::run_init_sequence(&self.hid)?;

        let (tx, rx) = mpsc::channel::<ChannelInfo>(64);
        let id = self.metadata.id.clone();
        let _ = tx.send(ChannelInfo::Connected(id)).await;

        let hid = self.hid.clone();
        let epoch = self.epoch;
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let join = std::thread::Builder::new()
            .name("steam-ctrl-reader".into())
            .spawn(move || {
                reader_loop(hid, tx, shutdown_rx, epoch);
            })
            .map_err(|e| DeviceError::Hid(e.to_string()))?;
        self.shutdown = Some(shutdown_tx);
        self.join = Some(join);

        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(s) = self.shutdown.take() {
            let _ = s.send(true);
        }
        if let Some(j) = self.join.take() {
            let _ = tokio::task::spawn_blocking(move || j.join()).await;
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        tracing::debug!(intensity, "steam controller rumble not yet implemented");
        Ok(())
    }
}

fn reader_loop(
    hid: Arc<StdMutex<HidDevice>>,
    tx: mpsc::Sender<ChannelInfo>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    epoch: Instant,
) {
    let mut buf = [0u8; 128];
    loop {
        match shutdown.has_changed() {
            // Explicit shutdown signal.
            Ok(true) if *shutdown.borrow_and_update() => break,
            // Sender dropped: treat as a shutdown request so the thread ends.
            Err(_) => break,
            _ => {}
        }
        let n = {
            let guard = hid.lock().unwrap();
            match guard.read_timeout(&mut buf, 50) {
                Ok(n) => n,
                Err(e) => {
                    tracing::debug!(error = %e, "steam ctrl read error");
                    break;
                }
            }
        };
        if n < STATE_BODY_LEN {
            continue;
        }
        // USB input reports prepend a 4-byte header; the actual state body
        // begins at byte 0 of buf when hidapi strips the leading report id.
        // If the first byte is non-zero (a "state" tag) we still slice from 0.
        let body = &buf[..n.min(buf.len())];
        let state = match parse_state(body) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let ts_us = epoch.elapsed().as_micros() as u64;
        let sample = ImuSample {
            gyro: [
                gyro_rad_s(state.gyro_raw[0]),
                gyro_rad_s(state.gyro_raw[1]),
                gyro_rad_s(state.gyro_raw[2]),
            ],
            accel: [
                accel_m_s2(state.accel_raw[0]),
                accel_m_s2(state.accel_raw[1]),
                accel_m_s2(state.accel_raw[2]),
            ],
            mag: None,
            timestamp_us: ts_us,
        };
        if tx
            .blocking_send(ChannelInfo::ImuSamples(vec![sample]))
            .is_err()
        {
            break;
        }
    }
    let _ = tx.blocking_send(ChannelInfo::Disconnected);
}
