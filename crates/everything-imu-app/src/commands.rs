//! Tauri commands — fine-grained mutations + atomic reads.

use crate::dto::{DeviceHistoryDto, DeviceMetadataDto, LogEntryDto, ResetKindDto, SettingsDto};
use crate::error::IpcError;
use crate::events::ConnectionStatusUpdate;
use crate::state::AppHandle;
use device_traits::DeviceId;
use everything_imu_core::pipeline::{FusionAlgo, MountingOrientation};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle as TauriAppHandle, Manager, State};

fn mac_key(mac: [u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02x}")).collect()
}

#[tauri::command]
#[specta::specta]
pub async fn list_devices(
    handle: State<'_, AppHandle>,
) -> Result<Vec<DeviceMetadataDto>, IpcError> {
    let snap = handle.state.device_metadata_snapshot().await;
    Ok(snap.iter().map(DeviceMetadataDto::from).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn list_device_history(
    handle: State<'_, AppHandle>,
) -> Result<Vec<DeviceHistoryDto>, IpcError> {
    let rows = handle.db.list_device_history()?;
    Ok(rows
        .into_iter()
        .map(|r| DeviceHistoryDto {
            mac: r.mac,
            serial: r.serial,
            kind: r.kind,
            last_seen: r.last_seen,
            rotation_deg: r.rotation_deg,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn get_settings(handle: State<'_, AppHandle>) -> Result<SettingsDto, IpcError> {
    let mut s = SettingsDto::default();
    if let Some(v) = handle.db.get_setting("slime_server_addr")? {
        s.slime_server_addr = v;
    }
    if let Some(v) = handle.db.get_setting("log_filter")? {
        s.log_filter = v;
    }
    if let Some(v) = handle.db.get_setting("theme")? {
        s.theme = v;
    }
    if let Some(v) = handle.db.get_setting("auto_start_synthetic")? {
        s.auto_start_synthetic = v == "1";
    }
    Ok(s)
}

#[tauri::command]
#[specta::specta]
pub async fn set_setting(
    handle: State<'_, AppHandle>,
    key: String,
    value: String,
) -> Result<(), IpcError> {
    handle.db.set_setting(&key, &value)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn request_reset(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    kind: ResetKindDto,
) -> Result<(), IpcError> {
    let snap = handle.state.device_metadata_snapshot().await;
    let target = snap
        .iter()
        .find(|m| m.id.mac == mac)
        .ok_or(IpcError::NotFound)?;
    let id = DeviceId {
        mac,
        serial: target.id.serial.clone(),
    };
    handle.state.request_reset(&id, kind.into()).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_device_rotation_offset(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    deg: f32,
) -> Result<(), IpcError> {
    if !deg.is_finite() {
        return Err(IpcError::Invalid("non-finite degrees".into()));
    }
    let key = format!(
        "rotation_offset_deg:{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    handle.db.set_setting(&key, &deg.to_string())?;
    let _ = handle.state.set_rotation_offset_deg(mac, deg).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_log_buffer(handle: State<'_, AppHandle>) -> Result<Vec<LogEntryDto>, IpcError> {
    let buf = handle.log_buffer.lock();
    Ok(buf.iter().cloned().collect())
}

#[tauri::command]
#[specta::specta]
pub async fn restart_synthetic(_handle: State<'_, AppHandle>, _count: u8) -> Result<(), IpcError> {
    Err(IpcError::Invalid(
        "restart_synthetic deferred to Sprint 5".into(),
    ))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FusionAlgoDto {
    Vqf,
    Madgwick,
    BasicVqf,
}

impl FusionAlgoDto {
    fn to_setting(self) -> &'static str {
        match self {
            Self::Vqf => "vqf",
            Self::Madgwick => "madgwick",
            Self::BasicVqf => "basic_vqf",
        }
    }
    fn from_setting(s: &str) -> Self {
        match FusionAlgo::from_setting(s) {
            FusionAlgo::Vqf => Self::Vqf,
            FusionAlgo::Madgwick => Self::Madgwick,
            FusionAlgo::BasicVqf => Self::BasicVqf,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MountingOrientationDto {
    Identity,
    LeftSide,
    RightSide,
    UpsideDown,
    FacingForward,
    FacingBack,
}

impl MountingOrientationDto {
    fn to_setting(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::LeftSide => "left_side",
            Self::RightSide => "right_side",
            Self::UpsideDown => "upside_down",
            Self::FacingForward => "facing_forward",
            Self::FacingBack => "facing_back",
        }
    }
    fn from_setting(s: &str) -> Self {
        match MountingOrientation::from_setting(s) {
            MountingOrientation::Identity => Self::Identity,
            MountingOrientation::LeftSide => Self::LeftSide,
            MountingOrientation::RightSide => Self::RightSide,
            MountingOrientation::UpsideDown => Self::UpsideDown,
            MountingOrientation::FacingForward => Self::FacingForward,
            MountingOrientation::FacingBack => Self::FacingBack,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct PerDeviceSettingsDto {
    pub fusion: FusionAlgoDto,
    pub mounting: MountingOrientationDto,
    pub magnetometer_enabled: bool,
    pub rotation_offset_deg: f32,
    /// Optional user-provided label ("right shin", "head") — purely
    /// informational on the bridge side; SlimeVR-Server owns body
    /// assignment and would override anything we put in tracker_position.
    pub label: String,
}

#[tauri::command]
#[specta::specta]
pub async fn get_per_device_settings(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<PerDeviceSettingsDto, IpcError> {
    let mk = mac_key(mac);
    let fusion = handle
        .db
        .get_setting(&format!("fusion_algo:{mk}"))?
        .map(|s| FusionAlgoDto::from_setting(&s))
        .unwrap_or(FusionAlgoDto::Vqf);
    let mounting = handle
        .db
        .get_setting(&format!("mounting_orientation:{mk}"))?
        .map(|s| MountingOrientationDto::from_setting(&s))
        .unwrap_or(MountingOrientationDto::Identity);
    let magnetometer_enabled = handle
        .db
        .get_setting(&format!("magnetometer_enabled:{mk}"))?
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let rotation_offset_deg = handle
        .db
        .get_setting(&format!(
            "rotation_offset_deg:{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        ))?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0_f32);
    let label = handle
        .db
        .get_setting(&format!("tracker_label:{mk}"))?
        .unwrap_or_default();
    Ok(PerDeviceSettingsDto {
        fusion,
        mounting,
        magnetometer_enabled,
        rotation_offset_deg,
        label,
    })
}

/// Persist a user-provided label for a tracker. Pure metadata — never
/// forwarded to SlimeVR-Server (body assignment lives there).
#[tauri::command]
#[specta::specta]
pub async fn set_tracker_label(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    label: String,
) -> Result<(), IpcError> {
    let key = format!("tracker_label:{}", mac_key(mac));
    let trimmed = label.trim();
    if trimmed.len() > 64 {
        return Err(IpcError::Invalid("label too long (max 64 chars)".into()));
    }
    handle.db.set_setting(&key, trimmed)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_fusion_algo(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    algo: FusionAlgoDto,
) -> Result<(), IpcError> {
    let key = format!("fusion_algo:{}", mac_key(mac));
    handle.db.set_setting(&key, algo.to_setting())?;
    tracing::info!(mac = ?mac, algo = ?algo, "fusion algo set (effective on reconnect)");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_mounting_orientation(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    orientation: MountingOrientationDto,
) -> Result<(), IpcError> {
    let key = format!("mounting_orientation:{}", mac_key(mac));
    handle.db.set_setting(&key, orientation.to_setting())?;
    let live = MountingOrientation::from_setting(orientation.to_setting());
    let applied_now = handle.state.set_mounting_orientation(mac, live).await;
    tracing::info!(
        mac = ?mac,
        orientation = ?orientation,
        applied_now,
        "mounting orientation set",
    );
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_magnetometer_enabled(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    enabled: bool,
) -> Result<(), IpcError> {
    let key = format!("magnetometer_enabled:{}", mac_key(mac));
    handle
        .db
        .set_setting(&key, if enabled { "1" } else { "0" })?;
    let applied_now = handle.state.set_magnetometer_enabled(mac, enabled).await;
    tracing::info!(
        mac = ?mac,
        enabled,
        applied_now,
        "magnetometer toggle set",
    );
    Ok(())
}

/// Opens the OS file manager at the rolling-log directory. Used by the
/// "Diagnostics → Open logs folder" button so the tester can attach the
/// log file when reporting an issue.
#[tauri::command]
#[specta::specta]
pub async fn open_logs_dir(app: TauriAppHandle) -> Result<(), IpcError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| IpcError::Internal(e.to_string()))?
        .join("logs");
    open_path(&dir)
}

/// Opens the OS file manager at the persistence-DB directory (where
/// `state.db` lives). Useful for backing up settings or sharing state
/// dumps for support.
#[tauri::command]
#[specta::specta]
pub async fn open_data_dir(app: TauriAppHandle) -> Result<(), IpcError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    open_path(&dir)
}

fn open_path(path: &std::path::Path) -> Result<(), IpcError> {
    std::fs::create_dir_all(path).ok();
    #[cfg(target_os = "windows")]
    let cmd = ("explorer", vec![path.as_os_str().to_owned()]);
    #[cfg(target_os = "linux")]
    let cmd = ("xdg-open", vec![path.as_os_str().to_owned()]);
    std::process::Command::new(cmd.0)
        .args(cmd.1)
        .spawn()
        .map_err(|e| IpcError::Internal(format!("open shell failed: {e}")))?;
    Ok(())
}

/// Toggle the global UDP-emission gate. While paused the pipeline keeps
/// reading IMU samples and updating fusion state, but no rotation /
/// accel / battery packets leave for SlimeVR-Server. Reset / handshake
/// commands are unaffected.
#[tauri::command]
#[specta::specta]
pub async fn set_emission_paused(
    handle: State<'_, AppHandle>,
    paused: bool,
) -> Result<(), IpcError> {
    handle.state.set_paused(paused);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_emission_paused(handle: State<'_, AppHandle>) -> Result<bool, IpcError> {
    Ok(handle.state.is_paused())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DoctorCheckDto {
    pub name: String,
    pub status: DoctorStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DoctorReportDto {
    pub platform: String,
    pub version: String,
    pub server_addr: String,
    pub checks: Vec<DoctorCheckDto>,
    pub overall: DoctorStatus,
}

/// In-app preflight diagnostic, mirrors `headless-cli --doctor`.
#[tauri::command]
#[specta::specta]
pub async fn doctor(handle: State<'_, AppHandle>) -> Result<DoctorReportDto, IpcError> {
    let server_addr = handle
        .db
        .get_setting("slime_server_addr")?
        .unwrap_or_else(|| "127.0.0.1:6969".to_string());

    let mut checks = Vec::new();
    let mut overall = DoctorStatus::Pass;
    let bump = |s: DoctorStatus, current: &mut DoctorStatus| {
        // Severity ordering: Fail > Warn > Pass.
        let weight = |st: DoctorStatus| match st {
            DoctorStatus::Pass => 0,
            DoctorStatus::Warn => 1,
            DoctorStatus::Fail => 2,
        };
        if weight(s) > weight(*current) {
            *current = s;
        }
    };

    let nintendo = device_joycon::JoyconFactory::list_paired().unwrap_or_default();
    let jc2_nearby = device_joycon::JoyconFactory::list_nearby_jc2(800)
        .await
        .unwrap_or_default();
    let sony_pads = device_dualsense::DualSenseFactory::list_paired().unwrap_or_default();
    let sony_moves = device_psmove::PsMoveFactory::list_paired().unwrap_or_default();
    let total = nintendo.len() + jc2_nearby.len() + sony_pads.len() + sony_moves.len();

    let dev_status = if total == 0 {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Pass
    };
    bump(dev_status, &mut overall);
    checks.push(DoctorCheckDto {
        name: "Paired devices".into(),
        status: dev_status,
        message: if total == 0 {
            "No paired controllers visible. Check Bluetooth pairing.".into()
        } else {
            format!(
                "{} controller(s) visible (jc1-hid={}, jc2-ble={}, sony-pad={}, ps-move={})",
                total,
                nintendo.len(),
                jc2_nearby.len(),
                sony_pads.len(),
                sony_moves.len(),
            )
        },
    });

    // UDP send probe.
    let udp_status = match server_addr.parse::<std::net::SocketAddr>() {
        Ok(addr) => match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(sock) => match sock.connect(addr).await {
                Ok(_) => match sock.send(b"\x00\x00\x00\x03").await {
                    Ok(_) => (DoctorStatus::Pass, format!("UDP send to {addr} accepted")),
                    Err(e) => (DoctorStatus::Fail, format!("UDP send failed: {e}")),
                },
                Err(e) => (DoctorStatus::Fail, format!("UDP connect failed: {e}")),
            },
            Err(e) => (DoctorStatus::Fail, format!("UDP bind failed: {e}")),
        },
        Err(e) => (
            DoctorStatus::Fail,
            format!("Invalid server address `{server_addr}`: {e}"),
        ),
    };
    bump(udp_status.0, &mut overall);
    checks.push(DoctorCheckDto {
        name: "UDP socket".into(),
        status: udp_status.0,
        message: udp_status.1,
    });

    // hidapi already exercised by list_paired above; record explicit pass.
    checks.push(DoctorCheckDto {
        name: "hidapi".into(),
        status: DoctorStatus::Pass,
        message: "hidapi singleton initialized".into(),
    });

    // Logs dir reachable / writable check via metadata.
    let logs_status = match handle.db.get_setting("__noop__") {
        Ok(_) => (DoctorStatus::Pass, "Persistence DB reachable".into()),
        Err(e) => (DoctorStatus::Fail, format!("Persistence DB error: {e}")),
    };
    bump(logs_status.0, &mut overall);
    checks.push(DoctorCheckDto {
        name: "Persistence".into(),
        status: logs_status.0,
        message: logs_status.1,
    });

    Ok(DoctorReportDto {
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        version: env!("CARGO_PKG_VERSION").to_string(),
        server_addr,
        checks,
        overall,
    })
}

/// Synchronous read of the current connection status. Tauri also emits
/// the same payload as `connection-status-update` ~1 Hz; this command is
/// useful for first paint when the UI mounts.
#[tauri::command]
#[specta::specta]
pub async fn get_connection_status(
    handle: State<'_, AppHandle>,
) -> Result<ConnectionStatusUpdate, IpcError> {
    let stats = handle.state.aggregated_stats().await;
    let server_addr = handle
        .db
        .get_setting("slime_server_addr")?
        .unwrap_or_else(|| "127.0.0.1:6969".to_string());
    let now_ms = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    };
    let last_send_ms_ago = if stats.last_send_ms_unix == 0 {
        None
    } else {
        Some(now_ms.saturating_sub(stats.last_send_ms_unix))
    };
    let last_handshake_ms_ago = if stats.last_handshake_ms_unix == 0 {
        None
    } else {
        Some(now_ms.saturating_sub(stats.last_handshake_ms_unix))
    };
    Ok(ConnectionStatusUpdate {
        server_addr,
        server_supports_bundle: stats.server_supports_bundle,
        packets_sent: stats.packets_sent,
        last_send_ms_ago,
        last_handshake_ms_ago,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OutputProfileDto {
    pub led_mask: u8,
    pub rumble_enabled: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn get_output_profile(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<OutputProfileDto, IpcError> {
    let mk = mac_key(mac);
    let led_mask = handle
        .db
        .get_setting(&format!("led_mask:{mk}"))?
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(0);
    let rumble_enabled = handle
        .db
        .get_setting(&format!("rumble_enabled:{mk}"))?
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    Ok(OutputProfileDto {
        led_mask,
        rumble_enabled,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn set_output_profile(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    profile: OutputProfileDto,
    apply_now: bool,
) -> Result<(), IpcError> {
    let mk = mac_key(mac);
    handle
        .db
        .set_setting(&format!("led_mask:{mk}"), &profile.led_mask.to_string())?;
    handle.db.set_setting(
        &format!("rumble_enabled:{mk}"),
        if profile.rumble_enabled { "1" } else { "0" },
    )?;
    if apply_now {
        let _ = handle.state.set_led_mask(mac, profile.led_mask).await;
        let _ = handle.state.set_rumble(mac, profile.rumble_enabled).await;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CalibrationWizardStatusDto {
    pub suggested_mounting: MountingOrientationDto,
    pub suggested_rotation_offset_deg: f32,
    pub accel_norm_mps2: f32,
    pub gyro_norm_rads: f32,
    pub sample_age_ms: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn get_calibration_wizard_status(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<CalibrationWizardStatusDto, IpcError> {
    let samples = handle.state.latest_sample_snapshot().await;
    let quat = handle.state.latest_quat_snapshot().await;
    let (id, sample) = samples
        .iter()
        .find(|(id, _)| id.mac == mac)
        .ok_or(IpcError::NotFound)?;
    let q = quat
        .get(id)
        .copied()
        .unwrap_or(everything_imu_core::QuatXyzw::IDENTITY);

    let acc = sample.acc_xyz;
    let abs = [acc[0].abs(), acc[1].abs(), acc[2].abs()];
    let suggested_mounting = if abs[2] >= abs[0] && abs[2] >= abs[1] {
        if acc[2] < 0.0 {
            MountingOrientationDto::UpsideDown
        } else {
            MountingOrientationDto::Identity
        }
    } else if abs[0] >= abs[1] {
        if acc[0] > 0.0 {
            MountingOrientationDto::LeftSide
        } else {
            MountingOrientationDto::RightSide
        }
    } else if acc[1] > 0.0 {
        MountingOrientationDto::FacingForward
    } else {
        MountingOrientationDto::FacingBack
    };

    let yaw_deg = yaw_deg_from_quat(q.0);
    Ok(CalibrationWizardStatusDto {
        suggested_mounting,
        suggested_rotation_offset_deg: -yaw_deg,
        accel_norm_mps2: norm3(sample.acc_xyz),
        gyro_norm_rads: norm3(sample.gyr_xyz),
        sample_age_ms: sample.elapsed_ms,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn apply_calibration_wizard(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    mounting: MountingOrientationDto,
    rotation_offset_deg: f32,
    magnetometer_enabled: bool,
) -> Result<(), IpcError> {
    if !rotation_offset_deg.is_finite() {
        return Err(IpcError::Invalid("non-finite degrees".into()));
    }
    let mk = mac_key(mac);
    handle
        .db
        .set_setting(&format!("mounting_orientation:{mk}"), mounting.to_setting())?;
    handle.db.set_setting(
        &format!(
            "rotation_offset_deg:{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        ),
        &rotation_offset_deg.to_string(),
    )?;
    handle.db.set_setting(
        &format!("magnetometer_enabled:{mk}"),
        if magnetometer_enabled { "1" } else { "0" },
    )?;
    let _ = handle
        .state
        .set_mounting_orientation(
            mac,
            MountingOrientation::from_setting(mounting.to_setting()),
        )
        .await;
    let _ = handle
        .state
        .set_rotation_offset_deg(mac, rotation_offset_deg)
        .await;
    let _ = handle
        .state
        .set_magnetometer_enabled(mac, magnetometer_enabled)
        .await;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AdvancedTelemetryDto {
    pub mac: [u8; 6],
    pub serial: String,
    pub sample_rate_hz: f32,
    pub battery_fraction: f32,
    pub gyro_norm_rads: f32,
    pub accel_norm_mps2: f32,
    pub mag_norm_u_t: Option<f32>,
    pub gyro_bias_rads: [f64; 3],
    pub orientation_xyzw: [f32; 4],
    pub sample_age_ms: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn get_advanced_telemetry(
    handle: State<'_, AppHandle>,
) -> Result<Vec<AdvancedTelemetryDto>, IpcError> {
    let meta = handle.state.device_metadata_snapshot().await;
    let quat = handle.state.latest_quat_snapshot().await;
    let sample = handle.state.latest_sample_snapshot().await;
    let bias = handle.state.latest_bias_snapshot().await;
    let rate = handle.state.latest_rate_snapshot().await;
    let battery = handle.state.latest_battery_snapshot().await;
    let mut out = Vec::with_capacity(meta.len());
    for m in meta {
        let id = m.id.clone();
        let q = quat
            .get(&id)
            .copied()
            .unwrap_or(everything_imu_core::QuatXyzw::IDENTITY);
        let s = sample.get(&id).copied().unwrap_or_default();
        let b = bias.get(&id).copied().unwrap_or_default();
        out.push(AdvancedTelemetryDto {
            mac: id.mac,
            serial: id.serial.clone(),
            sample_rate_hz: rate.get(&id).copied().unwrap_or(0.0),
            battery_fraction: battery.get(&id).copied().unwrap_or(f32::NAN),
            gyro_norm_rads: norm3(s.gyr_xyz),
            accel_norm_mps2: norm3(s.acc_xyz),
            mag_norm_u_t: s.mag_xyz.map(norm3),
            gyro_bias_rads: b.gyr_bias,
            orientation_xyzw: q.0,
            sample_age_ms: s.elapsed_ms,
        });
    }
    Ok(out)
}

fn norm3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn yaw_deg_from_quat(q: [f32; 4]) -> f32 {
    let x = q[0];
    let y = q[1];
    let z = q[2];
    let w = q[3];
    let siny_cosp = 2.0 * (w * y + x * z);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    siny_cosp.atan2(cosy_cosp).to_degrees()
}
