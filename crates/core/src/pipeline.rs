//! Pipeline — per-device tokio task consuming ChannelInfo and forwarding to SlimeVR-Server.

use crate::error::AppError;
use crate::quat::QuatXyzw;
use device_traits::{BiasStore, ChannelInfo, DeviceMetadata, ResetKind};
use imu_fusion::{BasicVqf, Madgwick, Vqf};
use imu_math::coord;
use nalgebra::Vector3;
use slime_tracker::client::SlimeClient;
use slime_tracker::{ActionType, SlimeQuaternion};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

/// Selectable orientation filter for the per-device pipeline.
///
/// Default: VQF (Versatile Quaternion-based Filter, Laidig 2023). Switching
/// algorithm currently requires a device reconnect — live-swap is queued
/// for a follow-up sprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FusionAlgo {
    #[default]
    Vqf,
    Madgwick,
    BasicVqf,
}

impl FusionAlgo {
    pub fn from_setting(s: &str) -> Self {
        match s {
            "madgwick" => Self::Madgwick,
            "basic_vqf" => Self::BasicVqf,
            _ => Self::Vqf,
        }
    }
    pub fn to_setting(self) -> &'static str {
        match self {
            Self::Vqf => "vqf",
            Self::Madgwick => "madgwick",
            Self::BasicVqf => "basic_vqf",
        }
    }
}

/// Discrete cardinal mounting orientations applied as a fixed quaternion
/// multiply on the outgoing rotation, before it leaves for SlimeVR-Server.
/// SlimeVR has a "mounting reset" that figures this out from gravity at
/// runtime — this enum is the manual override path for users who already
/// know how their tracker is strapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MountingOrientation {
    #[default]
    Identity,
    /// Tracker rotated 90° around the body Y axis (left side).
    LeftSide,
    /// Tracker rotated -90° around the body Y axis (right side).
    RightSide,
    /// Tracker rotated 180° around the body Y axis (worn upside down).
    UpsideDown,
    /// Tracker rotated 90° around the body X axis (forward-facing).
    FacingForward,
    /// Tracker rotated -90° around the body X axis (back-facing).
    FacingBack,
}

impl MountingOrientation {
    pub fn from_setting(s: &str) -> Self {
        match s {
            "left_side" => Self::LeftSide,
            "right_side" => Self::RightSide,
            "upside_down" => Self::UpsideDown,
            "facing_forward" => Self::FacingForward,
            "facing_back" => Self::FacingBack,
            _ => Self::Identity,
        }
    }
    pub fn to_setting(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::LeftSide => "left_side",
            Self::RightSide => "right_side",
            Self::UpsideDown => "upside_down",
            Self::FacingForward => "facing_forward",
            Self::FacingBack => "facing_back",
        }
    }

    /// Quaternion (xyzw) representing the mounting offset. Applied as
    /// `q_out = q_mount * q_estimate` before the rotation is forwarded.
    pub fn quat_xyzw(self) -> [f32; 4] {
        let half = std::f32::consts::FRAC_PI_2;
        let (axis, angle) = match self {
            Self::Identity => return [0.0, 0.0, 0.0, 1.0],
            Self::LeftSide => ([0.0, 1.0, 0.0], half),
            Self::RightSide => ([0.0, 1.0, 0.0], -half),
            Self::UpsideDown => ([0.0, 1.0, 0.0], std::f32::consts::PI),
            Self::FacingForward => ([1.0, 0.0, 0.0], half),
            Self::FacingBack => ([1.0, 0.0, 0.0], -half),
        };
        let s = (angle * 0.5).sin();
        let c = (angle * 0.5).cos();
        [axis[0] * s, axis[1] * s, axis[2] * s, c]
    }
}

/// Configuration provided to a freshly-spawned pipeline. All values are
/// derived from per-device settings stored in the persistence DB.
#[derive(Debug, Clone, Copy, Default)]
pub struct PipelineConfig {
    pub fusion: FusionAlgo,
    pub mounting: MountingOrientation,
    pub magnetometer_enabled: bool,
    /// Continuous yaw offset (degrees) applied to the outgoing rotation
    /// quaternion *after* the mounting preset. Useful for fine-tuning a
    /// tracker that's mounted slightly off-angle. Live-swappable.
    pub rotation_offset_deg: f32,
}

/// Internal: enum-dispatched orientation filter. Kept private so consumers
/// only see the [`FusionAlgo`] selector — the `update` / `quat_wijk` /
/// `bias_estimate` interface is uniform across implementations.
enum FilterImpl {
    Vqf(Vqf),
    Madgwick(Madgwick),
    BasicVqf(BasicVqf),
}

impl FilterImpl {
    fn new(algo: FusionAlgo, gyr_ts_s: f64) -> Self {
        match algo {
            FusionAlgo::Vqf => {
                let params = imu_fusion::vqf::VqfParams {
                    tau_acc: 100.0,
                    rest_bias_est_enabled: true,
                    rest_min_t: 3.0,
                    motion_bias_est_enabled: false,
                    ..imu_fusion::vqf::VqfParams::default()
                };

                Self::Vqf(imu_fusion::vqf::Vqf::with_params(gyr_ts_s, params))
            }
            FusionAlgo::Madgwick => Self::Madgwick(Madgwick::new(gyr_ts_s as f32)),
            FusionAlgo::BasicVqf => Self::BasicVqf(BasicVqf::new(gyr_ts_s)),
        }
    }

    fn set_bias_estimate(&mut self, bias: [f64; 3], _sigma: Option<f64>) {
        if let Self::Vqf(f) = self {
            f.set_bias_estimate(bias, _sigma);
        }
    }

    fn update(&mut self, gyr: [f64; 3], acc: [f64; 3], mag: Option<[f64; 3]>) {
        match self {
            Self::Vqf(f) => f.update(gyr, acc, mag),
            Self::Madgwick(f) => {
                if let Some(m) = mag {
                    f.update_marg(
                        gyr[0] as f32,
                        gyr[1] as f32,
                        gyr[2] as f32,
                        acc[0] as f32,
                        acc[1] as f32,
                        acc[2] as f32,
                        m[0] as f32,
                        m[1] as f32,
                        m[2] as f32,
                    );
                } else {
                    f.update_imu(
                        gyr[0] as f32,
                        gyr[1] as f32,
                        gyr[2] as f32,
                        acc[0] as f32,
                        acc[1] as f32,
                        acc[2] as f32,
                    );
                }
            }
            Self::BasicVqf(f) => f.update(gyr, acc),
        }
    }

    fn quat_wijk(&self, use_mag: bool) -> [f64; 4] {
        match self {
            Self::Vqf(f) => {
                if use_mag && f.mag_seen() {
                    f.quat_9d()
                } else {
                    f.quat_6d()
                }
            }
            Self::Madgwick(f) => {
                let q = f.quaternion();
                [q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64]
            }
            Self::BasicVqf(f) => f.quat_6d(),
        }
    }

    fn bias_estimate(&self) -> [f64; 3] {
        match self {
            Self::Vqf(f) => f.bias_estimate().0,
            _ => [0.0; 3],
        }
    }
}

/// Last raw IMU sample observed by the pipeline. Published via a `watch`
/// channel so the Tauri layer can poll at its own cadence (no per-sample IPC
/// flood). Frame matches the device-native body frame at the time of read
/// — coordinate remap to fusion / SlimeVR happens later in the pipeline.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImuSampleSnapshot {
    pub gyr_xyz: [f32; 3],
    pub acc_xyz: [f32; 3],
    pub mag_xyz: Option<[f32; 3]>,
    /// `Instant`-based monotonic millis since the pipeline started. Used
    /// only for client-side ordering — wall-clock timestamps are added at
    /// emit time.
    pub elapsed_ms: u64,
}

/// Live VQF gyro-bias estimate (rad/s) for diagnostics. Distinct from the
/// persisted bias snapshot in [`BiasStore`].
#[derive(Debug, Clone, Copy, Default)]
pub struct BiasSnapshot {
    pub gyr_bias: [f64; 3],
}

pub struct Pipeline {
    meta: DeviceMetadata,
    slime: Arc<SlimeClient>,
    bias_store: Arc<dyn BiasStore>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    fusion: FilterImpl,
    config_rx: watch::Receiver<PipelineConfig>,
    sensor_id: u8,
    last_persist: Instant,
    started_at: Instant,
    quat_tx: watch::Sender<QuatXyzw>,
    sample_tx: watch::Sender<ImuSampleSnapshot>,
    bias_tx: watch::Sender<BiasSnapshot>,
    rate_tx: watch::Sender<f32>,
    battery_tx: watch::Sender<f32>,
    rate_counter: VecDeque<Instant>,
    sensor_info_sent: bool,
    last_sensor_mag_config: Option<u16>,
}

pub struct PipelineHandles {
    pub quat_rx: watch::Receiver<QuatXyzw>,
    pub sample_rx: watch::Receiver<ImuSampleSnapshot>,
    pub bias_rx: watch::Receiver<BiasSnapshot>,
    pub rate_rx: watch::Receiver<f32>,
    pub battery_rx: watch::Receiver<f32>,
    /// Sender for live config updates. Mutating mounting / mag here takes
    /// effect on the next IMU batch without restarting the pipeline.
    /// Fusion algo changes here are recorded for the diagnostics layer
    /// but do *not* swap the running filter — that needs a reconnect to
    /// avoid dropping fusion state mid-stream.
    pub config_tx: watch::Sender<PipelineConfig>,
}

impl Pipeline {
    pub fn new(
        meta: DeviceMetadata,
        slime: Arc<SlimeClient>,
        bias_store: Arc<dyn BiasStore>,
        paused: Arc<std::sync::atomic::AtomicBool>,
        sensor_id: u8,
        config: PipelineConfig,
    ) -> (Self, PipelineHandles) {
        let gyr_ts = 1.0 / meta.capabilities.native_imu_rate_hz as f64;
        let mut fusion = FilterImpl::new(config.fusion, gyr_ts);
        if let Some(bias) = bias_store.load_bias(&meta.id) {
            fusion.set_bias_estimate(bias, Some(0.01));
            tracing::info!(
                id = %meta.id,
                algo = config.fusion.to_setting(),
                "seeded fusion bias from store",
            );
        }
        tracing::info!(
            id = %meta.id,
            mounting = config.mounting.to_setting(),
            mag_enabled = config.magnetometer_enabled,
            "pipeline configured",
        );
        let (quat_tx, quat_rx) = watch::channel(QuatXyzw::IDENTITY);
        let (sample_tx, sample_rx) = watch::channel(ImuSampleSnapshot::default());
        let (bias_tx, bias_rx) = watch::channel(BiasSnapshot::default());
        let (rate_tx, rate_rx) = watch::channel(0.0_f32);
        let (battery_tx, battery_rx) = watch::channel(f32::NAN);
        let (config_tx, config_rx) = watch::channel(config);
        let pipeline = Self {
            meta,
            slime,
            bias_store,
            paused,
            fusion,
            config_rx,
            sensor_id,
            last_persist: Instant::now(),
            started_at: Instant::now(),
            quat_tx,
            sample_tx,
            bias_tx,
            rate_tx,
            battery_tx,
            rate_counter: VecDeque::with_capacity(256),
            sensor_info_sent: false,
            last_sensor_mag_config: None,
        };
        let handles = PipelineHandles {
            quat_rx,
            sample_rx,
            bias_rx,
            rate_rx,
            battery_rx,
            config_tx,
        };
        (pipeline, handles)
    }

    pub async fn run(
        mut self,
        mut events: mpsc::Receiver<ChannelInfo>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<(), AppError> {
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                Some(evt) = events.recv() => {
                    if let Err(e) = self.handle_event(evt).await {
                        tracing::warn!(error = %e, "event handle failed; pipeline exiting");
                        self.persist_bias();
                        return Err(e);
                    }
                }
                else => break,
            }
        }
        self.persist_bias();
        Ok(())
    }

    fn sensor_mag_config(&self) -> u16 {
        if !self.meta.capabilities.has_magnetometer {
            0x0000
        } else if self.config_rx.borrow().magnetometer_enabled {
            0x0003
        } else {
            0x0002
        }
    }

    async fn send_sensor_info(&self, mag_config: u16) -> Result<(), AppError> {
        use slime_tracker::client::SensorDescriptor;
        use slime_tracker::{ImuType, TrackerDataType, TrackerPosition};

        let desc = SensorDescriptor {
            sensor_id: self.sensor_id,
            // Use Bno085 to tell SlimeVR Server that the data is already fused
            // and it should NOT apply its own server-side Drift Compensation.
            imu_type: ImuType::Bno085,
            mag_config,
            position: TrackerPosition::None,
            data_type: TrackerDataType::Rotation,
        };
        self.slime
            .send_sensor_info(&desc)
            .await
            .map_err(|e| AppError::Slime(e.to_string()))
    }

    async fn handle_event(&mut self, evt: ChannelInfo) -> Result<(), AppError> {
        let confirmed = self.slime.stats().handshake_confirmed;
        if !confirmed {
            self.sensor_info_sent = false;
            self.last_sensor_mag_config = None;
        } else {
            let desired_mag_config = self.sensor_mag_config();
            if !self.sensor_info_sent || self.last_sensor_mag_config != Some(desired_mag_config) {
                if let Err(e) = self.send_sensor_info(desired_mag_config).await {
                    tracing::warn!(error = %e, "sensor_info send failed");
                } else {
                    self.sensor_info_sent = true;
                    self.last_sensor_mag_config = Some(desired_mag_config);
                    tracing::debug!("sensor_info sent after handshake confirmed");
                }
            }
        }

        match evt {
            ChannelInfo::Connected(_) => {}
            ChannelInfo::ImuSamples(samples) => {
                if samples.is_empty() {
                    return Ok(());
                }
                let cfg = *self.config_rx.borrow();
                for s in &samples {
                    let gyro_vqf =
                        coord::jsl_to_vqf_body(Vector3::new(s.gyro[0], s.gyro[1], s.gyro[2]));
                    let accel_vqf =
                        coord::jsl_to_vqf_body(Vector3::new(s.accel[0], s.accel[1], s.accel[2]));
                    let mag_vqf = if cfg.magnetometer_enabled {
                        s.mag
                            .map(|m| coord::jsl_to_vqf_body(Vector3::new(m[0], m[1], m[2])))
                    } else {
                        None
                    };
                    self.fusion.update(gyro_vqf, accel_vqf, mag_vqf);
                }
                let q6 = self.fusion.quat_wijk(cfg.magnetometer_enabled);
                let q_estimate = QuatXyzw::from_vqf_wijk(q6);
                // Live-read mounting orientation + rotation offset each
                // batch so command-side changes apply without a reconnect.
                let mut q_xyzw_arr = q_estimate.0;
                if cfg.mounting != MountingOrientation::Identity {
                    q_xyzw_arr = quat_mul_xyzw(cfg.mounting.quat_xyzw(), q_xyzw_arr);
                }
                if cfg.rotation_offset_deg.abs() > f32::EPSILON {
                    q_xyzw_arr =
                        quat_mul_xyzw(yaw_offset_quat(cfg.rotation_offset_deg), q_xyzw_arr);
                }
                let q_xyzw = QuatXyzw(q_xyzw_arr);
                let _ = self.quat_tx.send(q_xyzw);

                // Publish raw sample + live bias for the diagnostics layer.
                // `watch` keeps only the latest, so the Tauri emitter polls
                // at its own cadence — no per-sample IPC.
                let last_raw = samples.last().expect("samples non-empty");
                let elapsed_ms = self.started_at.elapsed().as_millis() as u64;
                let _ = self.sample_tx.send(ImuSampleSnapshot {
                    gyr_xyz: last_raw.gyro,
                    acc_xyz: last_raw.accel,
                    mag_xyz: last_raw.mag,
                    elapsed_ms,
                });
                let bias = self.fusion.bias_estimate();
                let _ = self.bias_tx.send(BiasSnapshot { gyr_bias: bias });

                // Rate counter: last 1 s sliding window. Each ImuSamples
                // event ships N samples — credit them all so a 200 Hz
                // controller actually reads as 200 Hz instead of being
                // capped to the event-arrival rate.
                let now = Instant::now();
                let n = samples.len();
                for _ in 0..n {
                    self.rate_counter.push_back(now);
                }
                while let Some(&t) = self.rate_counter.front() {
                    if now.saturating_duration_since(t) > Duration::from_secs(1) {
                        self.rate_counter.pop_front();
                    } else {
                        break;
                    }
                }
                let _ = self.rate_tx.send(self.rate_counter.len() as f32);

                let slime_q = SlimeQuaternion {
                    i: q_xyzw.0[0],
                    j: q_xyzw.0[1],
                    k: q_xyzw.0[2],
                    w: q_xyzw.0[3],
                };
                let last = samples.last().unwrap();
                let accel_tuple = (last.accel[0], last.accel[1], last.accel[2]);
                if !self.paused.load(std::sync::atomic::Ordering::Acquire) {
                    self.slime
                        .send_rotation_and_accel(self.sensor_id, slime_q, accel_tuple)
                        .await
                        .map_err(|e| AppError::Slime(e.to_string()))?;
                    if cfg.magnetometer_enabled {
                        if let Some(m) = last.mag {
                            self.slime
                                .send_magnetometer(self.sensor_id, (m[0], m[1], m[2]))
                                .await
                                .map_err(|e| AppError::Slime(e.to_string()))?;
                        }
                    }
                }

                if self.last_persist.elapsed() >= Duration::from_secs(10) {
                    self.persist_bias();
                    self.last_persist = Instant::now();
                }
            }
            ChannelInfo::Battery(b) => {
                let _ = self.battery_tx.send(b.fraction);
                if !self.paused.load(std::sync::atomic::Ordering::Acquire) {
                    self.slime
                        .send_battery(0.0, b.fraction)
                        .await
                        .map_err(|e| AppError::Slime(e.to_string()))?;
                }
            }
            ChannelInfo::ResetRequested(kind) => {
                let action = match kind {
                    ResetKind::Yaw => ActionType::ResetYaw,
                    ResetKind::Full => ActionType::ResetFull,
                    ResetKind::Mounting => ActionType::ResetMounting,
                };
                self.slime
                    .send_user_action(action)
                    .await
                    .map_err(|e| AppError::Slime(e.to_string()))?;
            }
            ChannelInfo::Disconnected => {
                self.persist_bias();
                return Err(AppError::DeviceDisconnected);
            }
        }
        Ok(())
    }

    fn persist_bias(&self) {
        let bias = self.fusion.bias_estimate();
        self.bias_store.store_bias(&self.meta.id, bias);
        tracing::debug!(id = %self.meta.id, "bias persisted");
    }
}

/// Build a unit quaternion (xyzw) representing a rotation around the
/// world-frame Y axis by `deg` degrees. Used to apply a per-device yaw
/// offset on top of the cardinal mounting orientation.
fn yaw_offset_quat(deg: f32) -> [f32; 4] {
    let half = (deg * std::f32::consts::PI / 180.0) * 0.5;
    [0.0, half.sin(), 0.0, half.cos()]
}

/// Hamilton quaternion product on (x, y, z, w)-ordered f32 quaternions.
/// Used to apply a fixed [`MountingOrientation`] to the fusion estimate
/// before the rotation packet leaves for SlimeVR-Server.
fn quat_mul_xyzw(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let (ax, ay, az, aw) = (a[0], a[1], a[2], a[3]);
    let (bx, by, bz, bw) = (b[0], b[1], b[2], b[3]);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn identity_mounting_quat_is_unit() {
        let q = MountingOrientation::Identity.quat_xyzw();
        assert_eq!(q, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn cardinal_mounting_quats_are_unit_norm() {
        for o in [
            MountingOrientation::Identity,
            MountingOrientation::LeftSide,
            MountingOrientation::RightSide,
            MountingOrientation::UpsideDown,
            MountingOrientation::FacingForward,
            MountingOrientation::FacingBack,
        ] {
            let q = o.quat_xyzw();
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            assert!(approx_eq(n, 1.0, 1e-6), "{o:?} norm {n}");
        }
    }

    #[test]
    fn upside_down_then_upside_down_is_identity() {
        let q = MountingOrientation::UpsideDown.quat_xyzw();
        let q2 = quat_mul_xyzw(q, q);
        // 180° + 180° around the same axis = identity (modulo sign flip on w).
        assert!(approx_eq(q2[0].abs(), 0.0, 1e-5));
        assert!(approx_eq(q2[1].abs(), 0.0, 1e-5));
        assert!(approx_eq(q2[2].abs(), 0.0, 1e-5));
        assert!(approx_eq(q2[3].abs(), 1.0, 1e-5));
    }

    #[test]
    fn quat_mul_identity_returns_other() {
        let id = [0.0, 0.0, 0.0, 1.0];
        let q = [0.1, 0.2, 0.3, 0.927];
        let result = quat_mul_xyzw(id, q);
        for i in 0..4 {
            assert!(approx_eq(result[i], q[i], 1e-6));
        }
    }

    #[test]
    fn fusion_algo_round_trips_through_setting_strings() {
        for algo in [FusionAlgo::Vqf, FusionAlgo::Madgwick, FusionAlgo::BasicVqf] {
            assert_eq!(FusionAlgo::from_setting(algo.to_setting()), algo);
        }
    }

    #[test]
    fn mounting_orientation_round_trips_through_setting_strings() {
        for o in [
            MountingOrientation::Identity,
            MountingOrientation::LeftSide,
            MountingOrientation::RightSide,
            MountingOrientation::UpsideDown,
            MountingOrientation::FacingForward,
            MountingOrientation::FacingBack,
        ] {
            assert_eq!(MountingOrientation::from_setting(o.to_setting()), o);
        }
    }
}
