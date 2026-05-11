//! AppState — owns SlimeClient + device registry + stores.

use crate::error::AppError;
use crate::pipeline::{
    BiasSnapshot, FusionAlgo, ImuSampleSnapshot, MountingOrientation, Pipeline, PipelineConfig,
};
use crate::quat::QuatXyzw;
use device_traits::{BiasStore, ChannelInfo, DeviceId, DeviceMetadata, SettingsStore};
use slime_tracker::client::{ClientStats, HandshakeInfo, SlimeClient};
use slime_tracker::{BoardType, ImuType, McuType};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, RwLock};

pub struct AppState {
    slime_addr: SocketAddr,
    pub settings: Arc<dyn SettingsStore>,
    pub bias_store: Arc<dyn BiasStore>,
    /// Global emission gate: when true, the pipeline still consumes IMU
    /// samples and updates the fusion state, but skips sending rotation /
    /// accel / battery to SlimeVR-Server. Useful as an emergency stop
    /// without killing the bridge process.
    pub paused: Arc<AtomicBool>,
    devices: RwLock<HashMap<DeviceId, DeviceHandle>>,
}

struct DeviceHandle {
    pub metadata: DeviceMetadata,
    pub slime: Arc<SlimeClient>,
    pub task: tokio::task::JoinHandle<Result<(), AppError>>,
    pub stop: watch::Sender<bool>,
    pub quat_rx: watch::Receiver<QuatXyzw>,
    pub sample_rx: watch::Receiver<ImuSampleSnapshot>,
    pub bias_rx: watch::Receiver<BiasSnapshot>,
    pub rate_rx: watch::Receiver<f32>,
    pub battery_rx: watch::Receiver<f32>,
    pub config_tx: watch::Sender<crate::pipeline::PipelineConfig>,
    pub control_tx: mpsc::Sender<DeviceControl>,
}

#[derive(Debug, Clone, Copy)]
pub enum DeviceControl {
    SetLedMask(u8),
    SetRumble(bool),
}

impl AppState {
    pub async fn new(
        slime_addr: SocketAddr,
        settings: Arc<dyn SettingsStore>,
        bias_store: Arc<dyn BiasStore>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            slime_addr,
            settings,
            bias_store,
            paused: Arc::new(AtomicBool::new(false)),
            devices: RwLock::new(HashMap::new()),
        })
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
        // so N devices = N independent trackers in the dashboard.
        let info = HandshakeInfo {
            board: BoardType::Custom,
            imu: ImuType::Bno085,
            mcu: McuType::Unknown,
            mag_status,
            firmware: "everything-imu 1.0.0-alpha".into(),
            mac_address: meta.id.mac,
        };
        let slime = Arc::new(
            SlimeClient::connect(self.slime_addr, &info)
                .await
                .map_err(|e| AppError::Slime(e.to_string()))?,
        );
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
        let task = tokio::spawn(pipeline.run(events, stop_rx));
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
                config_tx: handles.config_tx,
                control_tx,
            },
        );
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
    pub async fn set_rotation_offset_deg(&self, mac: [u8; 6], deg: f32) -> bool {
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
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        h.control_tx
            .send(DeviceControl::SetLedMask(mask))
            .await
            .is_ok()
    }

    pub async fn set_rumble(&self, mac: [u8; 6], on: bool) -> bool {
        let devices = self.devices.read().await;
        let Some(h) = devices.values().find(|h| h.metadata.id.mac == mac) else {
            return false;
        };
        h.control_tx
            .send(DeviceControl::SetRumble(on))
            .await
            .is_ok()
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
        };
        for h in devices.values() {
            let s = h.slime.stats();
            agg.packets_sent += s.packets_sent;
            agg.last_send_ms_unix = agg.last_send_ms_unix.max(s.last_send_ms_unix);
            agg.last_handshake_ms_unix = agg.last_handshake_ms_unix.max(s.last_handshake_ms_unix);
            agg.last_inbound_ms_unix = agg.last_inbound_ms_unix.max(s.last_inbound_ms_unix);
            agg.server_supports_bundle |= s.server_supports_bundle;
            agg.handshake_confirmed |= s.handshake_confirmed;
        }
        agg
    }

    pub async fn shutdown(&self) {
        let mut devices = self.devices.write().await;
        for (_, h) in devices.drain() {
            let _ = h.stop.send(true);
            let _ = h.task.await;
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
        // Broadcast reset to all active device connections (same as C# v0.4.1).
        let devices = self.devices.read().await;
        for h in devices.values() {
            if let Err(e) = h.slime.send_user_action(action.clone()).await {
                tracing::warn!(id = %h.metadata.id, error = %e, "reset broadcast failed");
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
    let magnetometer_enabled = settings
        .get(&format!("magnetometer_enabled:{mac_key}"))
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let rotation_offset_deg = settings.get_rotation_offset_deg(id);
    PipelineConfig {
        fusion,
        mounting,
        magnetometer_enabled,
        rotation_offset_deg,
    }
}
