//! AppState — owns SlimeClient + device registry + stores.

use crate::error::AppError;
use crate::latency::LatencySnapshot;
use crate::lazy_slime::LazySlime;
use crate::pipeline::{
    BiasSnapshot, FusionAlgo, ImuSampleSnapshot, MagCalCommand, MagCalProgress,
    MountingOrientation, Pipeline, PipelineConfig,
};
use crate::quat::QuatXyzw;
use device_traits::{BiasStore, ChannelInfo, DeviceId, DeviceMetadata, SettingsStore};
use imu_math::mag_cal::MagCalibration;
use slime_tracker::client::{ClientStats, HandshakeInfo};
use slime_tracker::{BoardType, ImuType, McuType};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch, RwLock};

pub struct AppState {
    slime_addr: SocketAddr,
    pub settings: Arc<dyn SettingsStore>,
    pub bias_store: Arc<dyn BiasStore>,
    /// Global emission gate: when true, the pipeline still consumes IMU
    /// samples and updates the fusion state, but skips sending rotation /
    /// accel / battery to SlimeVR-Server. Useful as an emergency stop
    /// without killing the bridge process.
    pub paused: Arc<AtomicBool>,
    devices: Arc<RwLock<HashMap<DeviceId, DeviceHandle>>>,
    /// Broadcasts the metadata of each device the moment it is registered.
    /// The app layer subscribes and forwards a `DeviceDiscovered` event to
    /// the UI, so the device store stays current after first paint.
    device_events_tx: broadcast::Sender<DeviceMetadata>,
}

struct DeviceHandle {
    pub metadata: DeviceMetadata,
    pub slime: Arc<LazySlime>,
    pub task: tokio::task::JoinHandle<Result<(), AppError>>,
    pub stop: watch::Sender<bool>,
    pub quat_rx: watch::Receiver<QuatXyzw>,
    pub sample_rx: watch::Receiver<ImuSampleSnapshot>,
    pub bias_rx: watch::Receiver<BiasSnapshot>,
    pub rate_rx: watch::Receiver<f32>,
    pub battery_rx: watch::Receiver<f32>,
    pub latency_rx: watch::Receiver<LatencySnapshot>,
    pub config_tx: watch::Sender<crate::pipeline::PipelineConfig>,
    pub control_tx: mpsc::Sender<DeviceControl>,
    pub mag_cal_cmd_tx: watch::Sender<MagCalCommand>,
    pub mag_cal_progress_rx: watch::Receiver<MagCalProgress>,
    pub mag_cal_result_rx: watch::Receiver<Option<MagCalibration>>,
}

#[derive(Debug, Clone, Copy)]
pub enum DeviceControl {
    SetLedMask(u8),
    /// Rumble intensity in `0.0..=1.0` (0.0 = off).
    SetRumble(f32),
}

impl AppState {
    pub async fn new(
        slime_addr: SocketAddr,
        settings: Arc<dyn SettingsStore>,
        bias_store: Arc<dyn BiasStore>,
    ) -> Result<Self, AppError> {
        let (device_events_tx, _) = broadcast::channel(16);
        Ok(Self {
            slime_addr,
            settings,
            bias_store,
            paused: Arc::new(AtomicBool::new(false)),
            devices: Arc::new(RwLock::new(HashMap::new())),
            device_events_tx,
        })
    }

    /// Subscribe to device-registration events. Each registered device's
    /// metadata is broadcast once, right after it enters the registry.
    pub fn subscribe_device_events(&self) -> broadcast::Receiver<DeviceMetadata> {
        self.device_events_tx.subscribe()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub async fn register_device(
        &self,
        meta: DeviceMetadata,
        events: mpsc::Receiver<ChannelInfo>,
        control_tx: mpsc::Sender<DeviceControl>,
    ) -> Result<(), AppError> {
        // A re-announce with the same MAC is the same physical device coming
        // back under a new serial (new IP or a recovered route) — replace the
        // stale entry instead of accumulating ghosts that still receive rumble.
        {
            let mut devices = self.devices.write().await;
            let stale: Vec<DeviceId> = devices
                .iter()
                .filter(|(did, h)| **did != meta.id && h.metadata.id.mac == meta.id.mac)
                .map(|(did, _)| did.clone())
                .collect();
            for did in stale {
                if let Some(h) = devices.remove(&did) {
                    let _ = h.stop.send(true);
                    h.task.abort();
                    tracing::info!(old = %did, new = %meta.id, "replacing stale device with same MAC");
                }
            }
        }
        let config = pipeline_config_from_settings(self.settings.as_ref(), &meta.id);
        let mag_status = if !meta.capabilities.has_magnetometer {
            0
        } else if config.magnetometer_enabled {
            2
        } else {
            1
        };
        // Each device gets its own UDP connection + handshake with its real
        // MAC address. SlimeVR-Server identifies trackers by source socket,
        // so N devices = N independent trackers in the dashboard. The
        // connection is lazy: it opens on the device's first pipeline event,
        // so haptics-only endpoints (which never emit events) never appear
        // to SlimeVR and never flap the connection watchdog.
        let info = HandshakeInfo {
            board: BoardType::Custom,
            imu: ImuType::Bno085,
            mcu: McuType::Unknown,
            mag_status,
            firmware: concat!("everything-imu ", env!("CARGO_PKG_VERSION")).into(),
            mac_address: meta.id.mac,
        };
        let slime = Arc::new(LazySlime::new(self.slime_addr, info));
        let (stop_tx, stop_rx) = watch::channel(false);
        // Always sensor_id 0 — each device is its own tracker connection.
        let sensor_id = 0u8;
        let (pipeline, handles) = Pipeline::new(
            meta.clone(),
            slime.clone(),
            self.bias_store.clone(),
            self.paused.clone(),
            sensor_id,
            config,
        );
        let id = meta.id.clone();
        let meta_event = meta.clone();
        let devices_for_reap = self.devices.clone();
        let reap_id = meta.id.clone();
        let task = tokio::spawn(async move {
            let result = pipeline.run(events, stop_rx).await;
            if matches!(result, Err(AppError::DeviceDisconnected)) {
                // Route died (REMOVE msg or stale sweep): drop the registry
                // entry so snapshots, stats, and the rumble path stop seeing
                // a ghost.
                devices_for_reap.write().await.remove(&reap_id);
                tracing::info!(id = %reap_id, "device disconnected; removed from registry");
            }
            result
        });
        self.devices.write().await.insert(
            id,
            DeviceHandle {
                metadata: meta,
                slime,
                task,
                stop: stop_tx,
                quat_rx: handles.quat_rx,
                sample_rx: handles.sample_rx,
                bias_rx: handles.bias_rx,
                rate_rx: handles.rate_rx,
                battery_rx: handles.battery_rx,
                latency_rx: handles.latency_rx,
                config_tx: handles.config_tx,
                control_tx,
                mag_cal_cmd_tx: handles.mag_cal_cmd_tx,
                mag_cal_progress_rx: handles.mag_cal_progress_rx,
                mag_cal_result_rx: handles.mag_cal_result_rx,
            },
        );
        // Notify the app layer so it can push a DeviceDiscovered event.
        // A send error only means no subscriber yet — harmless.
        let _ = self.device_events_tx.send(meta_event);
        Ok(())
    }

    /// Live-update mounting orientation for a single device. Takes effect
    /// on the next IMU batch — no reconnect needed.
    pub async fn set_mounting_orientation(
        &self,
        mac: [u8; 6],
        mounting: MountingOrientation,
    ) -> bool {
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        let mut cfg = *h.config_tx.borrow();
        cfg.mounting = mounting;
        h.config_tx.send_replace(cfg);
        true
    }

    /// Live-update magnetometer enable for a single device. Takes effect
    /// on the next IMU batch.
    pub async fn set_magnetometer_enabled(&self, mac: [u8; 6], enabled: bool) -> bool {
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        let mut cfg = *h.config_tx.borrow();
        cfg.magnetometer_enabled = enabled;
        h.config_tx.send_replace(cfg);
        true
    }

    /// Live-update yaw rotation offset (degrees). Takes effect on the
    /// next IMU batch.
    /// Live-update the per-device gyro scale. Takes effect on the next IMU
    /// batch — no reconnect needed.
    pub async fn set_gyro_scale(&self, mac: [u8; 6], scale: f32) -> bool {
        if !scale.is_finite() || scale <= 0.0 {
            return false;
        }
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        let mut cfg = *h.config_tx.borrow();
        cfg.gyro_scale = scale;
        h.config_tx.send_replace(cfg);
        true
    }

    pub async fn set_rotation_offset_deg(&self, mac: [u8; 6], deg: f32) -> bool {
        if !deg.is_finite() {
            return false;
        }
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        let mut cfg = *h.config_tx.borrow();
        cfg.rotation_offset_deg = deg;
        h.config_tx.send_replace(cfg);
        true
    }

    pub async fn set_led_mask(&self, mac: [u8; 6], mask: u8) -> bool {
        let tx = {
            let devices = self.devices.read().await;
            let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
                return false;
            };
            h.control_tx.clone()
        };
        tx.send(DeviceControl::SetLedMask(mask)).await.is_ok()
    }

    pub async fn set_rumble(&self, mac: [u8; 6], intensity: f32) -> bool {
        let tx = {
            let devices = self.devices.read().await;
            let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
                return false;
            };
            h.control_tx.clone()
        };
        tx.send(DeviceControl::SetRumble(intensity)).await.is_ok()
    }

    /// Begin a magnetometer hard-iron calibration session for one device.
    /// The pipeline collects raw mag samples until [`Self::finish_mag_calibration`]
    /// or [`Self::cancel_mag_calibration`]. Returns false if the device is gone.
    pub async fn start_mag_calibration(&self, mac: [u8; 6]) -> bool {
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        h.mag_cal_cmd_tx.send(MagCalCommand::Start).is_ok()
    }

    /// Abort an in-progress calibration session, discarding collected samples.
    pub async fn cancel_mag_calibration(&self, mac: [u8; 6]) -> bool {
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        h.mag_cal_cmd_tx.send(MagCalCommand::Cancel).is_ok()
    }

    /// Finish a calibration session: signal the pipeline to fit a sphere and
    /// await the result. `None` means no such device, a send failure, the fit
    /// failed (too few / poorly-spread samples), or the pipeline did not
    /// respond within the timeout.
    pub async fn finish_mag_calibration(&self, mac: [u8; 6]) -> Option<MagCalibration> {
        let (cmd_tx, mut result_rx) = {
            let devices = self.devices.read().await;
            let h = devices.values().find(|h| h.metadata.id.mac == mac)?;
            (h.mag_cal_cmd_tx.clone(), h.mag_cal_result_rx.clone())
        };
        cmd_tx.send(MagCalCommand::Finish).ok()?;
        // The pipeline fits the sphere on its next loop iteration and publishes
        // the result. Bound the wait so a stalled pipeline can't hang the UI.
        tokio::time::timeout(Duration::from_secs(2), result_rx.changed())
            .await
            .ok()?
            .ok()?;
        let cal = *result_rx.borrow();
        cal
    }

    /// Latest calibration progress for a device, or `None` if it is gone.
    pub async fn mag_cal_progress(&self, mac: [u8; 6]) -> Option<MagCalProgress> {
        let devices = self.devices.read().await;
        devices
            .values()
            .find(|h| h.metadata.id.mac == mac)
            .map(|h| *h.mag_cal_progress_rx.borrow())
    }

    /// Live-update the magnetometer calibration applied by a device's pipeline.
    /// Takes effect on the next IMU batch — no reconnect needed.
    pub async fn set_mag_calibration(&self, mac: [u8; 6], cal: Option<MagCalibration>) -> bool {
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        let mut cfg = *h.config_tx.borrow();
        cfg.mag_calibration = cal;
        h.config_tx.send_replace(cfg);
        true
    }

    /// Aggregate connection stats across all per-device SlimeClients.
    /// Packets-sent is summed, timestamps take the most recent, and
    /// server_supports_bundle is true if any device's connection confirmed it.
    pub async fn aggregated_stats(&self) -> ClientStats {
        let devices = self.devices.read().await;
        let mut agg = ClientStats {
            packets_sent: 0,
            last_send_ms_unix: 0,
            last_handshake_ms_unix: 0,
            server_supports_bundle: false,
            handshake_confirmed: false,
            last_inbound_ms_unix: 0,
            handshake_reset_count: 0,
            last_reset_ms_unix: 0,
        };
        for h in devices.values() {
            // Lazily-connected devices that never streamed have no client —
            // they must not contribute (stale) stats to the aggregate.
            let Some(slime) = h.slime.peek() else {
                continue;
            };
            let s = slime.stats();
            agg.packets_sent += s.packets_sent;
            agg.last_send_ms_unix = agg.last_send_ms_unix.max(s.last_send_ms_unix);
            agg.last_handshake_ms_unix = agg.last_handshake_ms_unix.max(s.last_handshake_ms_unix);
            agg.last_inbound_ms_unix = agg.last_inbound_ms_unix.max(s.last_inbound_ms_unix);
            agg.server_supports_bundle |= s.server_supports_bundle;
            agg.handshake_confirmed |= s.handshake_confirmed;
            agg.handshake_reset_count = agg
                .handshake_reset_count
                .saturating_add(s.handshake_reset_count);
            agg.last_reset_ms_unix = agg.last_reset_ms_unix.max(s.last_reset_ms_unix);
        }
        agg
    }

    pub async fn shutdown(&self) {
        // Drain into an owned Vec and release the write guard before awaiting the
        // task joins, so the registry lock isn't held across the timeouts.
        let handles: Vec<_> = {
            let mut devices = self.devices.write().await;
            devices.drain().collect()
        };
        for (id, h) in handles {
            let _ = h.stop.send(true);
            // Bound the wait: a hung pipeline (stuck in UDP send, locked
            // driver thread) must not block global app shutdown.
            match tokio::time::timeout(Duration::from_secs(2), h.task).await {
                Ok(_) => {}
                Err(_) => {
                    tracing::warn!(id = %id, "pipeline task did not exit within 2s; abandoning")
                }
            }
        }
    }

    pub async fn device_metadata_snapshot(&self) -> Vec<DeviceMetadata> {
        self.devices
            .read()
            .await
            .values()
            .map(|h| h.metadata.clone())
            .collect()
    }

    pub async fn latest_quat_snapshot(&self) -> HashMap<DeviceId, QuatXyzw> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), *h.quat_rx.borrow()))
            .collect()
    }

    pub async fn latest_sample_snapshot(&self) -> HashMap<DeviceId, ImuSampleSnapshot> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), *h.sample_rx.borrow()))
            .collect()
    }

    pub async fn latest_bias_snapshot(&self) -> HashMap<DeviceId, BiasSnapshot> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), *h.bias_rx.borrow()))
            .collect()
    }

    pub async fn latest_rate_snapshot(&self) -> HashMap<DeviceId, f32> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), *h.rate_rx.borrow()))
            .collect()
    }

    pub async fn latest_latency_snapshot(&self) -> HashMap<DeviceId, LatencySnapshot> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), *h.latency_rx.borrow()))
            .collect()
    }

    pub async fn latest_battery_snapshot(&self) -> HashMap<DeviceId, f32> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(id, h)| (id.clone(), *h.battery_rx.borrow()))
            .collect()
    }

    pub async fn request_reset(
        &self,
        _id: &DeviceId,
        kind: device_traits::ResetKind,
    ) -> Result<(), AppError> {
        let action = match kind {
            device_traits::ResetKind::Yaw => slime_tracker::ActionType::ResetYaw,
            device_traits::ResetKind::Full => slime_tracker::ActionType::ResetFull,
            device_traits::ResetKind::Mounting => slime_tracker::ActionType::ResetMounting,
        };
        // Broadcast reset to all active device connections (same as SlimeIMU v0.4.x).
        // Collect live connections under the lock, then send outside it so the
        // read guard isn't held across the per-device UDP awaits.
        let targets: Vec<_> = {
            let devices = self.devices.read().await;
            devices
                .values()
                // Only devices that actually stream have a live connection; a
                // reset wouldn't mean anything to SlimeVR for the rest anyway.
                .filter_map(|h| h.slime.peek().cloned().map(|s| (h.metadata.id.clone(), s)))
                .collect()
        };
        for (id, slime) in targets {
            if let Err(e) = slime.send_user_action(action.clone()).await {
                tracing::warn!(id = %id, error = %e, "reset broadcast failed");
            }
        }
        Ok(())
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        tracing::info!("AppState dropping");
    }
}

/// Lets the OSC haptic bridge drive device rumble without depending on
/// `core`'s internals — it only sees the [`RumbleSink`] trait.
#[async_trait::async_trait]
impl osc_haptics::RumbleSink for AppState {
    async fn set_rumble(&self, mac: [u8; 6], intensity: f32) {
        let _ = AppState::set_rumble(self, mac, intensity).await;
    }
}

/// Read per-device pipeline settings from the [`SettingsStore`]. Keys are
/// scoped by lower-cased MAC. Missing or invalid values fall back to
/// [`PipelineConfig::default()`] (VQF, identity mounting, no magnetometer).
fn pipeline_config_from_settings(settings: &dyn SettingsStore, id: &DeviceId) -> PipelineConfig {
    let mac_key = id
        .mac
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let fusion = settings
        .get(&format!("fusion_algo:{mac_key}"))
        .map(|s| FusionAlgo::from_setting(&s))
        .unwrap_or_default();
    let mounting = settings
        .get(&format!("mounting_orientation:{mac_key}"))
        .map(|s| MountingOrientation::from_setting(&s))
        .unwrap_or_default();
    // A persisted hard-iron calibration only exists for a device that has a
    // magnetometer, so its presence is a sufficient auto-enable signal — no
    // need to consult device capabilities here.
    let mag_calibration = settings.get(&format!("mag_cal:{mac_key}")).and_then(|json| {
        match serde_json::from_str::<MagCalibration>(&json) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "stored mag calibration unparseable; ignoring");
                None
            }
        }
    });
    // Auto-enable the magnetometer once a calibration exists. An explicit
    // `magnetometer_enabled` setting still overrides (the user can calibrate
    // and then deliberately turn it off).
    let magnetometer_enabled = settings
        .get(&format!("magnetometer_enabled:{mac_key}"))
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(mag_calibration.is_some());
    let rotation_offset_deg = settings.get_rotation_offset_deg(id);
    let gyro_scale = settings
        .get(&format!("gyro_scale:{mac_key}"))
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0);
    PipelineConfig {
        fusion,
        mounting,
        magnetometer_enabled,
        rotation_offset_deg,
        gyro_scale,
        mag_calibration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use device_traits::{
        ChannelInfo, DeviceCapabilities, DeviceKind, InMemoryBiasStore, InMemorySettingsStore,
    };

    fn meta(mac: [u8; 6], serial: &str) -> DeviceMetadata {
        DeviceMetadata {
            id: DeviceId {
                mac,
                serial: serial.into(),
            },
            kind: DeviceKind::Phone,
            firmware: None,
            capabilities: DeviceCapabilities {
                has_magnetometer: false,
                has_battery: false,
                has_rumble: true,
                native_imu_rate_hz: 200,
            },
        }
    }

    async fn state() -> AppState {
        AppState::new(
            "127.0.0.1:9".parse().unwrap(),
            Arc::new(InMemorySettingsStore::default()),
            Arc::new(InMemoryBiasStore::default()),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn disconnected_device_is_reaped_from_registry() {
        let st = state().await;
        let (ev_tx, ev_rx) = mpsc::channel(4);
        let (ctl_tx, _ctl_rx) = mpsc::channel(4);
        st.register_device(meta([1, 2, 3, 4, 5, 6], "a"), ev_rx, ctl_tx)
            .await
            .unwrap();
        assert_eq!(st.device_metadata_snapshot().await.len(), 1);

        ev_tx.send(ChannelInfo::Disconnected).await.unwrap();
        drop(ev_tx);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if st.device_metadata_snapshot().await.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("device was never reaped from the registry");
    }

    #[tokio::test]
    async fn reannounce_with_same_mac_replaces_stale_device() {
        let st = state().await;
        let (_tx1, rx1) = mpsc::channel(4);
        let (ctl1, _c1) = mpsc::channel(4);
        st.register_device(meta([9, 9, 9, 9, 9, 9], "old-serial"), rx1, ctl1)
            .await
            .unwrap();
        let (_tx2, rx2) = mpsc::channel(4);
        let (ctl2, _c2) = mpsc::channel(4);
        st.register_device(meta([9, 9, 9, 9, 9, 9], "new-serial"), rx2, ctl2)
            .await
            .unwrap();
        let snap = st.device_metadata_snapshot().await;
        assert_eq!(snap.len(), 1, "same-MAC device must replace, not duplicate");
        assert_eq!(snap[0].id.serial, "new-serial");
    }
}
