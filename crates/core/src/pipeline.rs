//! Pipeline — per-device tokio task consuming ChannelInfo and forwarding to SlimeVR-Server.

use crate::error::AppError;
use crate::latency::{LatencySnapshot, LatencyTracker};
use crate::quat::QuatXyzw;
use device_traits::{BiasStore, ChannelInfo, DeviceMetadata, ResetKind};
use imu_fusion::{BasicVqf, Madgwick, Vqf, VqfParams};
use imu_math::coord;
use imu_math::mag_cal::{self, MagCalibration};
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
#[derive(Debug, Clone, Copy)]
pub struct PipelineConfig {
    pub fusion: FusionAlgo,
    pub mounting: MountingOrientation,
    pub magnetometer_enabled: bool,
    /// Continuous yaw offset (degrees) applied to the outgoing rotation
    /// quaternion *after* the mounting preset. Useful for fine-tuning a
    /// tracker that's mounted slightly off-angle. Live-swappable.
    pub rotation_offset_deg: f32,
    /// Per-device multiplicative scale applied to the raw gyroscope vector
    /// *before* it reaches the fusion filter. 1.0 is no-op; lower values
    /// soften overshoot on hot units, higher values compensate for
    /// firmware that under-reports angular rate. Range clamped in the UI
    /// to roughly 0.5..=2.0; values outside that are still respected but
    /// likely indicate a calibration problem.
    pub gyro_scale: f32,
    /// Hard-iron calibration for the magnetometer. When `None`, the magnetometer
    /// is *not* fed to fusion even if `magnetometer_enabled` is true — an
    /// uncalibrated magnetometer reads worse than no magnetometer at all.
    pub mag_calibration: Option<MagCalibration>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            fusion: FusionAlgo::default(),
            mounting: MountingOrientation::default(),
            magnetometer_enabled: false,
            rotation_offset_deg: 0.0,
            gyro_scale: 1.0,
            mag_calibration: None,
        }
    }
}

/// Edge-triggered command driving a magnetometer calibration session. Sent
/// over a `watch` channel; the pipeline acts on each transition away from
/// [`MagCalCommand::Idle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MagCalCommand {
    #[default]
    Idle,
    /// Begin collecting raw magnetometer samples.
    Start,
    /// Stop collecting and fit a [`MagCalibration`] from the buffer.
    Finish,
    /// Stop collecting and discard the buffer.
    Cancel,
}

/// Live progress of a magnetometer calibration session, published every IMU
/// batch while a session is active so the UI can render a coverage meter.
#[derive(Debug, Clone, Copy, Default)]
pub struct MagCalProgress {
    pub active: bool,
    pub n_samples: u32,
    /// Direction-bin coverage `0.0..=1.0` — drives the "rotate the device"
    /// progress ring. A figure-8 through all orientations approaches 1.0.
    pub coverage: f32,
    /// Provisional fitted field magnitude (µT), 0.0 before enough samples.
    pub field_strength_ut: f32,
}

/// Upper bound on buffered calibration samples. At ~62 Hz (Joy-Con 2) this is
/// ~64 s of capture — far more than a calibration needs; extra samples are
/// dropped rather than growing the buffer unbounded.
const MAG_CAL_BUFFER_CAP: usize = 4000;

/// Internal: enum-dispatched orientation filter. Kept private so consumers
/// only see the [`FusionAlgo`] selector — the `update` / `quat_wijk` /
/// `bias_estimate` interface is uniform across implementations.
enum FilterImpl {
    Vqf(Vqf),
    Madgwick(Madgwick),
    BasicVqf(BasicVqf),
}

impl FilterImpl {
    fn new(algo: FusionAlgo, gyr_ts_s: f64, has_mag: bool) -> Self {
        match algo {
            FusionAlgo::Vqf => {
                // VQF gyro-bias estimation has two paths: a rest path (gyro
                // directly observes bias on all 3 axes while still) and a
                // motion path (a Kalman estimator driven only by accelerometer
                // inclination correction while moving). The motion path can
                // only observe the pitch/roll bias components — rotation about
                // gravity leaves the accelerometer invariant, so on a 6-DOF
                // device with NO magnetometer the yaw/heading gyro bias is
                // mathematically unobservable. In that mode VQF only nudges the
                // heading bias toward zero with a weak artificial pseudo-
                // measurement, and any systematic error in the observable axes
                // (e.g. a wrong effective sample dt from a Bluetooth adapter
                // that delivers HID reports in bursts) leaks into the
                // unobservable heading channel and rails the estimate to
                // biasClip. That railed bias is subtracted from the gyro every
                // sample, producing a constant phantom yaw rate — the tracked
                // limb slides toward the direction of motion ("ice-skating"),
                // survives resets, and re-accumulates on the next movement.
                //
                // So: enable the motion path ONLY when a magnetometer makes the
                // heading axis observable (9-DOF). For 6-DOF (no-mag)
                // controllers, run rest-bias only — it still corrects all three
                // axes whenever the device is genuinely still (controllers
                // strapped to a limb micro-rest constantly), and it removes the
                // sole mechanism that can inject a wrong heading bias. This
                // matches the validated C# legacy, which ran stock VQF plus a
                // separate rest-only gyro-bias calibrator.
                //
                // biasClip stays at VQF's stock 2 deg/s. (A previous build
                // raised it to 5 to let the rest detector fire on controllers
                // whose true bias sits near the 2 deg/s cap — e.g. a DualSense
                // yaw axis with a zero factory-cal byte. With the motion path
                // disabled, widening the clip only raises the worst-case rail,
                // so instead that case is handled by seeding the bias from the
                // persisted rest calibration at connect, which pulls the live
                // at-rest reading back under the 2 deg/s gate.)
                let params = VqfParams {
                    motion_bias_est_enabled: has_mag,
                    rest_bias_est_enabled: true,
                    ..VqfParams::default()
                };
                Self::Vqf(Vqf::with_params(gyr_ts_s, params))
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

    /// Update the fusion timestep live, preserving the orientation estimate.
    /// Called by the pipeline once it has measured the device's real delivered
    /// sample cadence (see [`Pipeline`] rate calibration) — the connect-time
    /// `native_imu_rate_hz` is only a provisional guess for Bluetooth devices
    /// whose true rate depends on the negotiated link interval.
    fn set_timestep(&mut self, gyr_ts: f64) {
        match self {
            Self::Vqf(f) => f.set_timestep(gyr_ts),
            Self::Madgwick(f) => f.set_sample_period(gyr_ts as f32),
            Self::BasicVqf(f) => f.set_timestep(gyr_ts),
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
    latency_tx: watch::Sender<LatencySnapshot>,
    latency: LatencyTracker,
    rate_counter: VecDeque<Instant>,
    sensor_info_sent: bool,
    last_sensor_mag_config: Option<u16>,
    mag_cal_cmd_rx: watch::Receiver<MagCalCommand>,
    mag_cal_progress_tx: watch::Sender<MagCalProgress>,
    mag_cal_result_tx: watch::Sender<Option<MagCalibration>>,
    mag_cal_buffer: Vec<[f32; 3]>,
    mag_cal_active: bool,
    /// Timestep currently baked into the fusion filter (seconds/sample).
    /// Seeded from `native_imu_rate_hz`; refined live by rate calibration.
    gyr_ts_current: f64,
    /// Wall-clock of the first IMU batch, to time out the connect-backlog
    /// warmup before rate counting begins.
    rate_cal_first_batch: Option<Instant>,
    /// Start of the current rate-count window.
    rate_cal_window_start: Option<Instant>,
    /// Samples delivered since the current window opened.
    rate_cal_window_samples: u64,
    /// EMA of the per-window throughput (Hz), smoothing the window-to-window
    /// jitter a contended link produces so the timestep tracks the sustained
    /// rate instead of chasing noise.
    rate_cal_ema_hz: Option<f64>,
}

pub struct PipelineHandles {
    pub quat_rx: watch::Receiver<QuatXyzw>,
    pub sample_rx: watch::Receiver<ImuSampleSnapshot>,
    pub bias_rx: watch::Receiver<BiasSnapshot>,
    pub rate_rx: watch::Receiver<f32>,
    pub battery_rx: watch::Receiver<f32>,
    pub latency_rx: watch::Receiver<LatencySnapshot>,
    /// Sender for live config updates. Mutating mounting / mag here takes
    /// effect on the next IMU batch without restarting the pipeline.
    /// Fusion algo changes here are recorded for the diagnostics layer
    /// but do *not* swap the running filter — that needs a reconnect to
    /// avoid dropping fusion state mid-stream.
    pub config_tx: watch::Sender<PipelineConfig>,
    /// Drives a magnetometer calibration session. See [`MagCalCommand`].
    pub mag_cal_cmd_tx: watch::Sender<MagCalCommand>,
    /// Live calibration progress while a session is active.
    pub mag_cal_progress_rx: watch::Receiver<MagCalProgress>,
    /// Carries the fitted [`MagCalibration`] after a [`MagCalCommand::Finish`].
    /// `None` means the fit failed (too few samples / poor geometry).
    pub mag_cal_result_rx: watch::Receiver<Option<MagCalibration>>,
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
        let mut fusion =
            FilterImpl::new(config.fusion, gyr_ts, meta.capabilities.has_magnetometer);
        if let Some(bias) = bias_store.load_bias(&meta.id) {
            // VQF's default biasClip is 2 deg/s. A stored bias whose magnitude
            // is at (or extremely near) that cap on any axis is almost
            // certainly saturated garbage from a previous session that ran
            // with a noisy gyro: VQF gave up at the clip ceiling, that value
            // got persisted on shutdown, and reseeding it locks the next
            // session at the same ceiling — a self-reinforcing loop that
            // produces phantom yaw drift on devices without a magnetometer.
            // Treat anything ≥ 1.9 deg/s as suspect and discard.
            // Real factory-calibrated gyro bias on these chips lives at <0.5 deg/s; the
            // VQF biasClip is 2 deg/s. Anything at or above 1.0 deg/s is treated as
            // saturation artifact, never a legitimate per-unit offset.
            const BIAS_CAP_DPS: f64 = 1.0;
            let bias_dps = [
                bias[0].to_degrees(),
                bias[1].to_degrees(),
                bias[2].to_degrees(),
            ];
            let saturated = bias_dps.iter().any(|v| v.abs() >= BIAS_CAP_DPS);
            if saturated {
                tracing::warn!(
                    id = %meta.id,
                    bias_dps = ?bias_dps,
                    cap_dps = BIAS_CAP_DPS,
                    "stored bias saturated near VQF biasClip; discarding to break self-reinforcing drift loop"
                );
            } else {
                fusion.set_bias_estimate(bias, Some(0.01));
                tracing::info!(
                    id = %meta.id,
                    algo = config.fusion.to_setting(),
                    bias_rad_s = ?bias,
                    bias_dps = ?bias_dps,
                    "seeded fusion bias from store",
                );
            }
        } else {
            tracing::info!(
                id = %meta.id,
                "no stored fusion bias; VQF starts from zero"
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
        let (latency_tx, latency_rx) = watch::channel(LatencySnapshot::default());
        let (config_tx, config_rx) = watch::channel(config);
        let (mag_cal_cmd_tx, mag_cal_cmd_rx) = watch::channel(MagCalCommand::Idle);
        let (mag_cal_progress_tx, mag_cal_progress_rx) = watch::channel(MagCalProgress::default());
        let (mag_cal_result_tx, mag_cal_result_rx) = watch::channel(None);
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
            latency_tx,
            latency: LatencyTracker::new(),
            rate_counter: VecDeque::with_capacity(256),
            sensor_info_sent: false,
            last_sensor_mag_config: None,
            mag_cal_cmd_rx,
            mag_cal_progress_tx,
            mag_cal_result_tx,
            mag_cal_buffer: Vec::new(),
            mag_cal_active: false,
            gyr_ts_current: gyr_ts,
            rate_cal_first_batch: None,
            rate_cal_window_start: None,
            rate_cal_window_samples: 0,
            rate_cal_ema_hz: None,
        };
        let handles = PipelineHandles {
            quat_rx,
            sample_rx,
            bias_rx,
            rate_rx,
            battery_rx,
            latency_rx,
            config_tx,
            mag_cal_cmd_tx,
            mag_cal_progress_rx,
            mag_cal_result_rx,
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
                _ = self.mag_cal_cmd_rx.changed() => {
                    self.handle_mag_cal_cmd();
                }
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
                let arrival = Instant::now();
                self.latency.record_arrival(arrival);
                self.calibrate_rate(samples.len(), arrival);
                let cfg = *self.config_rx.borrow();
                for s in &samples {
                    // Collect raw magnetometer samples for an in-progress
                    // calibration session, in the device body frame — the
                    // same frame the fitted offset is later subtracted in.
                    if self.mag_cal_active {
                        if let Some(m) = s.mag {
                            if self.mag_cal_buffer.len() < MAG_CAL_BUFFER_CAP {
                                self.mag_cal_buffer.push(m);
                            }
                        }
                    }
                    let gs = cfg.gyro_scale;
                    let gyro_vqf = coord::jsl_to_vqf_body(Vector3::new(
                        s.gyro[0] * gs,
                        s.gyro[1] * gs,
                        s.gyro[2] * gs,
                    ));
                    let accel_vqf =
                        coord::jsl_to_vqf_body(Vector3::new(s.accel[0], s.accel[1], s.accel[2]));
                    // Feed the magnetometer to fusion only when enabled AND
                    // calibrated — an uncalibrated hard-iron offset corrupts
                    // yaw worse than plain 6D gyro drift.
                    let mag_vqf = match (cfg.magnetometer_enabled, cfg.mag_calibration) {
                        (true, Some(cal)) => s.mag.map(|m| {
                            coord::jsl_to_vqf_body(Vector3::new(
                                m[0] - cal.offset[0],
                                m[1] - cal.offset[1],
                                m[2] - cal.offset[2],
                            ))
                        }),
                        _ => None,
                    };
                    self.fusion.update(gyro_vqf, accel_vqf, mag_vqf);
                }
                self.publish_mag_cal_progress();
                let q6 = self.fusion.quat_wijk(cfg.magnetometer_enabled);
                let q_estimate = QuatXyzw::from_vqf_wijk(q6);
                // Live-read mounting orientation + rotation offset each
                // batch so command-side changes apply without a reconnect.
                let mut q_xyzw_arr = q_estimate.0;
                let mut mutated = false;
                if cfg.mounting != MountingOrientation::Identity {
                    q_xyzw_arr = quat_mul_xyzw(cfg.mounting.quat_xyzw(), q_xyzw_arr);
                    mutated = true;
                }
                // Threshold in degrees (not f32::EPSILON which is ~1e-7 rad-
                // equivalent — far below any visible rotation).
                if cfg.rotation_offset_deg.abs() > 1e-4 {
                    q_xyzw_arr =
                        quat_mul_xyzw(yaw_offset_quat(cfg.rotation_offset_deg), q_xyzw_arr);
                    mutated = true;
                }
                // Re-normalize only when we actually composed. f64→f32 cast
                // plus repeated quaternion products accumulate ‖q‖ drift; an
                // un-normalized rotation propagates as a non-rigid transform
                // downstream in SlimeVR-Server.
                if mutated {
                    let n2 = q_xyzw_arr[0] * q_xyzw_arr[0]
                        + q_xyzw_arr[1] * q_xyzw_arr[1]
                        + q_xyzw_arr[2] * q_xyzw_arr[2]
                        + q_xyzw_arr[3] * q_xyzw_arr[3];
                    if n2 > 0.0 {
                        let inv = 1.0 / n2.sqrt();
                        q_xyzw_arr[0] *= inv;
                        q_xyzw_arr[1] *= inv;
                        q_xyzw_arr[2] *= inv;
                        q_xyzw_arr[3] *= inv;
                    }
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
                // Rotation packet rides in the VQF/SlimeVR frame; the accel
                // sibling must match — emitting raw device-frame accel
                // alongside a remapped quaternion gives the server a
                // mismatched gravity vector and breaks tilt-based features.
                let accel_vqf_last = coord::jsl_to_vqf_body(Vector3::new(
                    last_raw.accel[0],
                    last_raw.accel[1],
                    last_raw.accel[2],
                ));
                let accel_tuple = (
                    accel_vqf_last[0] as f32,
                    accel_vqf_last[1] as f32,
                    accel_vqf_last[2] as f32,
                );
                if !self.paused.load(std::sync::atomic::Ordering::Acquire) {
                    let send_start = Instant::now();
                    self.slime
                        .send_rotation_and_accel(self.sensor_id, slime_q, accel_tuple)
                        .await
                        .map_err(|e| AppError::Slime(e.to_string()))?;
                    if cfg.magnetometer_enabled {
                        if let Some(m) = last_raw.mag {
                            // Apply hard-iron correction before forwarding —
                            // matches what the local fusion ingests, so any
                            // server-side consumer of the mag channel sees a
                            // calibrated field, not a raw biased one.
                            let (mx, my, mz) = match cfg.mag_calibration {
                                Some(cal) => (
                                    m[0] - cal.offset[0],
                                    m[1] - cal.offset[1],
                                    m[2] - cal.offset[2],
                                ),
                                None => (m[0], m[1], m[2]),
                            };
                            self.slime
                                .send_magnetometer(self.sensor_id, (mx, my, mz))
                                .await
                                .map_err(|e| AppError::Slime(e.to_string()))?;
                        }
                    }
                    self.latency.record_send(send_start.elapsed());
                }

                // Publish latency snapshot every batch — receivers are
                // `watch` channels so emitter only reads the latest at 1 Hz.
                let _ = self.latency_tx.send(self.latency.snapshot());

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

    /// Live, continuously-adaptive fusion-timestep calibration from real
    /// delivery throughput.
    ///
    /// The fusion filter integrates one step of `gyr_ts_current` per `update()`
    /// call, and we call `update()` once per delivered sample — so for the
    /// filter's internal clock to track wall-clock, `gyr_ts_current` must equal
    /// `1 / (samples delivered per second)`. For Bluetooth HID devices that
    /// throughput is set by the negotiated link interval, not the sensor ODR,
    /// and is both unknown at connect (the connect burst makes a connect-time
    /// probe read garbage) and free to shift mid-session when the stack
    /// renegotiates the sniff interval under load. So measure it continuously:
    ///
    /// **Count samples over a wall-clock window** (`delivered ÷ window_seconds`),
    /// NOT inter-report deltas. On Linux hidraw the kernel buffers reports and
    /// hands them back in sub-millisecond clumps; any delta-based statistic
    /// (mean or median) is then dominated by either the intra-clump zeros or the
    /// inter-clump gaps and mis-estimates the true rate — a count over a fixed
    /// window is immune because it only depends on the total and the elapsed
    /// time, exactly like the on-screen rate meter. The first [`WARMUP_SECS`]
    /// are skipped so the connect backlog drains before counting, then each
    /// [`WINDOW_SECS`] window yields a rate and the timestep is re-applied
    /// whenever it has drifted more than [`APPLY_THRESHOLD`] from the live value
    /// (the 3 %% hysteresis stops steady-state windows from thrashing the filter;
    /// each apply resets only the bias covariance, leaving the estimate intact).
    fn calibrate_rate(&mut self, n_samples: usize, arrival: Instant) {
        // Plausibility band for the measured per-sample period: 0.8 ms (1250 Hz)
        // to 25 ms (40 Hz). Covers every supported device (Joy-Con, Joy-Con 2,
        // DualSense, PS Move, Wii) and clamps a degenerate window.
        const MIN_TS: f64 = 1.0 / 1250.0;
        const MAX_TS: f64 = 1.0 / 40.0;
        // Connect-backlog drain skipped before counting opens.
        const WARMUP_SECS: f64 = 1.5;
        // Count window length: long enough to average burst/gap clumping into a
        // steady throughput, short enough to track a real mid-session shift.
        const WINDOW_SECS: f64 = 4.0;
        // EMA weight on each new window. Low enough that one noisy window can't
        // swing the applied timestep; a sustained shift still converges over a
        // few windows.
        const EMA_ALPHA: f64 = 0.25;
        // Minimum relative gap between the smoothed rate and the live timestep
        // worth applying. Wide enough that the residual window-to-window jitter
        // of a contended link does not thrash the filter (each apply resets the
        // bias covariance, so re-applying every window would stop the bias
        // estimate from ever settling).
        const APPLY_THRESHOLD: f64 = 0.05;

        if n_samples == 0 {
            return;
        }
        let first = *self.rate_cal_first_batch.get_or_insert(arrival);
        if arrival.duration_since(first).as_secs_f64() < WARMUP_SECS {
            return;
        }
        let window_start = *self.rate_cal_window_start.get_or_insert(arrival);
        self.rate_cal_window_samples += n_samples as u64;
        let elapsed = arrival.duration_since(window_start).as_secs_f64();
        if elapsed < WINDOW_SECS {
            return;
        }

        let window_samples = self.rate_cal_window_samples;
        let rate_hz = window_samples as f64 / elapsed;
        self.rate_cal_window_start = Some(arrival);
        self.rate_cal_window_samples = 0;

        // Smooth the throughput before acting on it.
        let ema_hz = match self.rate_cal_ema_hz {
            Some(prev) => EMA_ALPHA * rate_hz + (1.0 - EMA_ALPHA) * prev,
            None => rate_hz,
        };
        self.rate_cal_ema_hz = Some(ema_hz);

        let measured_ts = (1.0 / ema_hz).clamp(MIN_TS, MAX_TS);
        if let Some(new_ts) = corrected_timestep(measured_ts, self.gyr_ts_current, APPLY_THRESHOLD) {
            self.fusion.set_timestep(new_ts);
            tracing::info!(
                id = %self.meta.id,
                from_rate_hz = 1.0 / self.gyr_ts_current,
                to_rate_hz = 1.0 / new_ts,
                window_rate_hz = rate_hz,
                window_samples,
                "fusion timestep recalibrated from live delivery throughput",
            );
            self.gyr_ts_current = new_ts;
        } else {
            // Always surface the measured throughput for diagnostics, even when
            // it confirms the current timestep (no change applied).
            tracing::debug!(
                id = %self.meta.id,
                window_rate_hz = rate_hz,
                ema_rate_hz = ema_hz,
                current_rate_hz = 1.0 / self.gyr_ts_current,
                "live delivery throughput measured; within threshold, no change",
            );
        }
    }

    fn persist_bias(&self) {
        let bias = self.fusion.bias_estimate();
        // Mirror the load-side guard: never persist a bias that is at or
        // near VQF's biasClip ceiling. Such values are saturation artifacts,
        // not real per-unit gyro offsets, and re-seeding them on the next
        // session would lock VQF at the cap and produce phantom yaw drift.
        // Real factory-calibrated gyro bias on these chips lives at <0.5 deg/s; the
        // VQF biasClip is 2 deg/s. Anything at or above 1.0 deg/s is treated as
        // saturation artifact, never a legitimate per-unit offset.
        const BIAS_CAP_DPS: f64 = 1.0;
        let saturated = bias.iter().any(|v| v.to_degrees().abs() >= BIAS_CAP_DPS);
        if saturated {
            tracing::debug!(
                id = %self.meta.id,
                bias_dps = ?[bias[0].to_degrees(), bias[1].to_degrees(), bias[2].to_degrees()],
                "skipping bias persistence: saturated estimate (not a real per-unit offset)"
            );
            return;
        }
        self.bias_store.store_bias(&self.meta.id, bias);
        tracing::debug!(id = %self.meta.id, "bias persisted");
    }

    /// React to a [`MagCalCommand`] transition. Start clears the buffer and
    /// arms collection; Finish fits a [`MagCalibration`] and publishes it on
    /// the result channel; Cancel discards the buffer.
    fn handle_mag_cal_cmd(&mut self) {
        let cmd = *self.mag_cal_cmd_rx.borrow_and_update();
        match cmd {
            MagCalCommand::Idle => {}
            MagCalCommand::Start => {
                self.mag_cal_buffer.clear();
                self.mag_cal_active = true;
                let _ = self.mag_cal_progress_tx.send(MagCalProgress {
                    active: true,
                    ..Default::default()
                });
                tracing::info!(id = %self.meta.id, "mag calibration session started");
            }
            MagCalCommand::Cancel => {
                self.mag_cal_active = false;
                self.mag_cal_buffer.clear();
                let _ = self.mag_cal_progress_tx.send(MagCalProgress::default());
                tracing::info!(id = %self.meta.id, "mag calibration session cancelled");
            }
            MagCalCommand::Finish => {
                self.mag_cal_active = false;
                let result = mag_cal::calibrate(&self.mag_cal_buffer);
                match &result {
                    Some(c) => tracing::info!(
                        id = %self.meta.id,
                        offset = ?c.offset,
                        coverage = c.coverage,
                        residual = c.residual,
                        field_ut = c.field_strength_ut,
                        "mag calibration fitted",
                    ),
                    None => tracing::warn!(
                        id = %self.meta.id,
                        n = self.mag_cal_buffer.len(),
                        "mag calibration fit failed",
                    ),
                }
                self.mag_cal_buffer.clear();
                let _ = self.mag_cal_progress_tx.send(MagCalProgress::default());
                let _ = self.mag_cal_result_tx.send(result);
            }
        }
    }

    /// Publish live calibration progress while a session is active. Re-fits
    /// the sphere each batch — a 4×4 solve, cheap relative to fusion.
    fn publish_mag_cal_progress(&self) {
        if !self.mag_cal_active {
            return;
        }
        let buf = &self.mag_cal_buffer;
        let (coverage, field_strength_ut) = match mag_cal::fit_sphere(buf) {
            Some(fit) => (mag_cal::coverage(buf, fit.center), fit.radius),
            None => (0.0, 0.0),
        };
        let _ = self.mag_cal_progress_tx.send(MagCalProgress {
            active: true,
            n_samples: buf.len() as u32,
            coverage,
            field_strength_ut,
        });
    }
}

/// Given a measured per-sample period and the timestep currently in the filter,
/// return the corrected timestep when they differ by more than `threshold`
/// (relative), else `None` (the live rate was already close enough).
fn corrected_timestep(measured_ts: f64, current_ts: f64, threshold: f64) -> Option<f64> {
    if measured_ts <= 0.0 || current_ts <= 0.0 {
        return None;
    }
    let rel = (measured_ts - current_ts).abs() / current_ts;
    (rel > threshold).then_some(measured_ts)
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
    fn corrected_timestep_applies_above_threshold() {
        // Assumed 200 Hz (5 ms), measured 270 Hz (~3.7 ms): 26% gap → correct.
        let assumed = 1.0 / 200.0;
        let measured = 1.0 / 270.0;
        assert_eq!(corrected_timestep(measured, assumed, 0.03), Some(measured));
    }

    #[test]
    fn corrected_timestep_skips_within_threshold() {
        // Measured 205 Hz vs assumed 200 Hz: 2.5% gap, under 3% → no change.
        let assumed = 1.0 / 200.0;
        let measured = 1.0 / 205.0;
        assert_eq!(corrected_timestep(measured, assumed, 0.03), None);
    }

    #[test]
    fn corrected_timestep_rejects_degenerate_input() {
        assert_eq!(corrected_timestep(0.0, 1.0 / 200.0, 0.03), None);
        assert_eq!(corrected_timestep(1.0 / 200.0, 0.0, 0.03), None);
    }

    #[test]
    fn set_timestep_preserves_orientation() {
        // Recalibrating the timestep must not jump the orientation estimate.
        use imu_fusion::Vqf;
        let mut f = Vqf::new(1.0 / 200.0);
        for _ in 0..50 {
            f.update([0.1, -0.2, 0.05], [0.0, 0.0, 9.81], None);
        }
        let before = f.quat_6d();
        f.set_timestep(1.0 / 270.0);
        let after = f.quat_6d();
        for i in 0..4 {
            assert!((before[i] - after[i]).abs() < 1e-9, "quat moved at {i}");
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
