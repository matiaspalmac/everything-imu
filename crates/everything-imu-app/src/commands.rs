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
    if let Some(v) = handle.db.get_setting("close_to_tray")? {
        s.close_to_tray = v == "1";
    }
    if let Some(v) = handle.db.get_setting("auto_update_on_startup")? {
        s.auto_update_on_startup = v != "0";
    }
    if let Some(v) = handle.db.get_setting("auto_install_on_startup")? {
        s.auto_install_on_startup = v != "0";
    }
    if let Some(v) = handle.db.get_setting("crash_report_enabled")? {
        s.crash_report_enabled = v == "1";
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
pub async fn restart_synthetic(handle: State<'_, AppHandle>, count: u8) -> Result<(), IpcError> {
    use device_dualsense::DualSenseFactory;
    use device_joycon::JoyconFactory;
    use device_psmove::PsMoveFactory;
    use device_traits::DeviceFactory;
    use everything_imu_core::Supervisor;
    use std::sync::Arc;

    if count == 0 {
        return Err(IpcError::Invalid("count must be >= 1".into()));
    }
    // Drop every registered device so the synthetic pool is the only
    // thing live afterwards. The original real-device factory loops
    // (started by supervisor_boot) keep polling in the background — they
    // simply won't find hardware to re-register on a dev box.
    handle.state.shutdown().await;

    let factories: Vec<Arc<dyn DeviceFactory>> = vec![
        Arc::new(JoyconFactory::synthetic(count)),
        Arc::new(DualSenseFactory::synthetic(count)),
        Arc::new(PsMoveFactory::synthetic(count)),
    ];
    let sup = Supervisor::new(handle.state.clone(), factories);
    tokio::spawn(async move {
        if let Err(e) = sup.run().await {
            tracing::warn!(error = %e, "synthetic supervisor exited");
        }
    });
    Ok(())
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
    /// Per-device multiplicative gyro scale (1.0 = identity). Persisted as
    /// the `gyro_scale:<mac>` setting; applied pre-fusion so any change
    /// shows up in the next IMU batch.
    pub gyro_scale: f32,
    /// Optional user-provided label ("right shin", "head") — purely
    /// informational on the bridge side; SlimeVR-Server owns body
    /// assignment and would override anything we put in tracker_position.
    pub label: String,
    /// When true, this tracker is hidden from the Dashboard list
    /// (Devices page still shows it with an unhide affordance). UI-only;
    /// pipeline keeps emitting to SlimeVR-Server.
    pub hidden: bool,
    /// Display order on the Dashboard / Devices pages. Lower sorts
    /// first; ties broken by insertion order. Set by drag-reorder UX.
    pub display_order: i32,
    /// Free-form group label ("L-leg", "R-arm") for visual grouping +
    /// broadcast-to-group resets. UI-only; SlimeVR-Server never sees it.
    pub group: String,
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
    // Mirror the pipeline's auto-enable rule: a device with a persisted
    // calibration defaults to magnetometer-on; an explicit setting overrides.
    let has_mag_cal = handle
        .db
        .get_setting(&mag_cal_key(mac))?
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let magnetometer_enabled = handle
        .db
        .get_setting(&format!("magnetometer_enabled:{mk}"))?
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(has_mag_cal);
    let rotation_offset_deg = handle
        .db
        .get_setting(&format!(
            "rotation_offset_deg:{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        ))?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0_f32);
    let gyro_scale = handle
        .db
        .get_setting(&format!("gyro_scale:{mk}"))?
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0_f32);
    let label = handle
        .db
        .get_setting(&format!("tracker_label:{mk}"))?
        .unwrap_or_default();
    let hidden = handle
        .db
        .get_setting(&format!("tracker_hidden:{mk}"))?
        .map(|s| s == "1")
        .unwrap_or(false);
    let display_order = handle
        .db
        .get_setting(&format!("tracker_order:{mk}"))?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0_i32);
    let group = handle
        .db
        .get_setting(&format!("tracker_group:{mk}"))?
        .unwrap_or_default();
    Ok(PerDeviceSettingsDto {
        fusion,
        mounting,
        magnetometer_enabled,
        rotation_offset_deg,
        gyro_scale,
        label,
        hidden,
        display_order,
        group,
    })
}

/// Persist + live-apply the per-device gyroscope scale. `scale` is a raw
/// multiplier (1.0 = identity); the UI clamps to a sane range before
/// invoking. Rejects non-finite / non-positive values rather than silently
/// disabling the gyroscope.
#[tauri::command]
#[specta::specta]
pub async fn set_gyro_scale(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    scale: f32,
) -> Result<(), IpcError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(IpcError::Invalid("gyro_scale must be finite and > 0".into()));
    }
    let key = format!("gyro_scale:{}", mac_key(mac));
    handle.db.set_setting(&key, &scale.to_string())?;
    handle.state.set_gyro_scale(mac, scale).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_tracker_hidden(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    hidden: bool,
) -> Result<(), IpcError> {
    let key = format!("tracker_hidden:{}", mac_key(mac));
    handle
        .db
        .set_setting(&key, if hidden { "1" } else { "0" })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_tracker_order(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    order: i32,
) -> Result<(), IpcError> {
    let key = format!("tracker_order:{}", mac_key(mac));
    handle.db.set_setting(&key, &order.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_tracker_group(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    group: String,
) -> Result<(), IpcError> {
    let trimmed = group.trim();
    if trimmed.len() > 32 {
        return Err(IpcError::Invalid("group too long (max 32 chars)".into()));
    }
    let key = format!("tracker_group:{}", mac_key(mac));
    handle.db.set_setting(&key, trimmed)?;
    Ok(())
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
        let _ = handle
            .state
            .set_rumble(mac, if profile.rumble_enabled { 1.0 } else { 0.0 })
            .await;
    }
    Ok(())
}

/// Pulse a controller's rumble motor briefly so a tester can confirm the
/// device receives commands. Clamped to 1500 ms server-side to avoid
/// runaway motors if the frontend forgets to send the stop. No-op if the
/// device is gone or has no rumble.
#[tauri::command]
#[specta::specta]
pub async fn test_rumble(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    duration_ms: u32,
) -> Result<(), IpcError> {
    test_rumble_at(handle, mac, 1.0, duration_ms).await
}

/// Pulse rumble at a specific intensity. Used by the haptic calibration
/// wizard, which steps through 0.1 .. 1.0 to find the user's perception
/// floor + ceiling. Intensity is clamped to [0, 1]; durations beyond
/// 1500ms are capped for the same runaway-motor reason as `test_rumble`.
#[tauri::command]
#[specta::specta]
pub async fn test_rumble_at(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    intensity: f32,
    duration_ms: u32,
) -> Result<(), IpcError> {
    let i = intensity.clamp(0.0, 1.0);
    let dur = std::time::Duration::from_millis(duration_ms.min(1500) as u64);
    if !handle.state.set_rumble(mac, i).await {
        return Ok(());
    }
    let state = handle.state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(dur).await;
        let _ = state.set_rumble(mac, 0.0).await;
    });
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub struct HapticCalibrationDto {
    /// Perception floor (0..1). Any intensity input <= floor is treated
    /// as silence — the motor needs more than this to be felt.
    pub floor: f32,
    /// Gain applied to intensities above the floor. 1.0 is identity;
    /// >1.0 pre-emphasizes weak signals, <1.0 dampens.
    pub gain: f32,
}

impl Default for HapticCalibrationDto {
    fn default() -> Self {
        Self { floor: 0.0, gain: 1.0 }
    }
}

/// Read the user's perception-calibrated rumble curve for a single
/// device. Returns identity (floor=0, gain=1) when nothing is stored.
#[tauri::command]
#[specta::specta]
pub async fn get_haptic_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<HapticCalibrationDto, IpcError> {
    let mk = mac_key(mac);
    let floor = handle
        .db
        .get_setting(&format!("haptic_floor:{mk}"))?
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        .unwrap_or(0.0);
    let gain = handle
        .db
        .get_setting(&format!("haptic_gain:{mk}"))?
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite() && (0.0..=4.0).contains(v))
        .unwrap_or(1.0);
    Ok(HapticCalibrationDto { floor, gain })
}

/// Persist the per-device haptic floor + gain. The runtime rumble path
/// reads these via the same settings store, so a change takes effect
/// on the next OSC-driven pulse without a reconnect.
#[tauri::command]
#[specta::specta]
pub async fn set_haptic_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    cal: HapticCalibrationDto,
) -> Result<(), IpcError> {
    if !cal.floor.is_finite() || !(0.0..=1.0).contains(&cal.floor) {
        return Err(IpcError::Invalid("floor must be in [0, 1]".into()));
    }
    if !cal.gain.is_finite() || !(0.0..=4.0).contains(&cal.gain) {
        return Err(IpcError::Invalid("gain must be in [0, 4]".into()));
    }
    let mk = mac_key(mac);
    handle
        .db
        .set_setting(&format!("haptic_floor:{mk}"), &cal.floor.to_string())?;
    handle
        .db
        .set_setting(&format!("haptic_gain:{mk}"), &cal.gain.to_string())?;
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

// --- magnetometer calibration ----------------------------------------------

/// DB key holding a device's persisted hard-iron calibration as a JSON blob.
fn mag_cal_key(mac: [u8; 6]) -> String {
    format!("mag_cal:{}", mac_key(mac))
}

/// Persisted hard-iron calibration, frontend-facing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub struct MagCalibrationDto {
    /// Hard-iron offset (µT) subtracted from raw magnetometer samples.
    pub offset: [f32; 3],
    /// Fitted field magnitude (µT). Earth's field is ~25-65 µT.
    pub field_strength_ut: f32,
    /// Sphere-fit RMS residual (µT) — lower is a tighter fit.
    pub residual: f32,
    /// Direction-bin coverage 0.0..=1.0 of the calibration sample set.
    pub coverage: f32,
}

impl From<imu_math::mag_cal::MagCalibration> for MagCalibrationDto {
    fn from(c: imu_math::mag_cal::MagCalibration) -> Self {
        Self {
            offset: c.offset,
            field_strength_ut: c.field_strength_ut,
            residual: c.residual,
            coverage: c.coverage,
        }
    }
}

/// Live progress of an in-flight calibration session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub struct MagCalProgressDto {
    pub active: bool,
    pub n_samples: u32,
    /// Coverage 0.0..=1.0 — drives the "rotate the device" progress ring.
    pub coverage: f32,
    pub field_strength_ut: f32,
}

/// Begin a magnetometer calibration session for one device. The user then
/// rotates the device through all orientations while the pipeline collects
/// samples; the UI polls [`get_mag_cal_progress`].
#[tauri::command]
#[specta::specta]
pub async fn start_mag_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<bool, IpcError> {
    Ok(handle.state.start_mag_calibration(mac).await)
}

/// Abort an in-progress calibration session, discarding collected samples.
#[tauri::command]
#[specta::specta]
pub async fn cancel_mag_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<bool, IpcError> {
    Ok(handle.state.cancel_mag_calibration(mac).await)
}

/// Finish a calibration session: fit the sphere, persist the result, enable
/// the magnetometer, and apply it to the running pipeline. Errors if the fit
/// failed (too few or poorly-spread samples — rotate more and retry).
#[tauri::command]
#[specta::specta]
pub async fn finish_mag_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<MagCalibrationDto, IpcError> {
    let cal = handle
        .state
        .finish_mag_calibration(mac)
        .await
        .ok_or_else(|| {
            IpcError::Internal(
                "magnetometer calibration fit failed — rotate the device through more \
                 orientations and try again"
                    .into(),
            )
        })?;
    let json = serde_json::to_string(&cal).map_err(|e| IpcError::Internal(e.to_string()))?;
    handle.db.set_setting(&mag_cal_key(mac), &json)?;
    // Calibrating implies wanting the magnetometer on — enable it unless the
    // user later turns it off explicitly.
    handle
        .db
        .set_setting(&format!("magnetometer_enabled:{}", mac_key(mac)), "1")?;
    handle.state.set_mag_calibration(mac, Some(cal)).await;
    handle.state.set_magnetometer_enabled(mac, true).await;
    tracing::info!(mac = ?mac, coverage = cal.coverage, residual = cal.residual, "mag calibration saved");
    Ok(cal.into())
}

/// Latest progress of an in-flight calibration session, or an inactive
/// snapshot if none is running.
#[tauri::command]
#[specta::specta]
pub async fn get_mag_cal_progress(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<MagCalProgressDto, IpcError> {
    let p = handle.state.mag_cal_progress(mac).await.unwrap_or_default();
    Ok(MagCalProgressDto {
        active: p.active,
        n_samples: p.n_samples,
        coverage: p.coverage,
        field_strength_ut: p.field_strength_ut,
    })
}

/// The persisted hard-iron calibration for a device, or `None` if uncalibrated.
#[tauri::command]
#[specta::specta]
pub async fn get_mag_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<Option<MagCalibrationDto>, IpcError> {
    let cal = handle
        .db
        .get_setting(&mag_cal_key(mac))?
        .and_then(|json| serde_json::from_str::<imu_math::mag_cal::MagCalibration>(&json).ok())
        .map(MagCalibrationDto::from);
    Ok(cal)
}

/// Discard a device's persisted calibration. The magnetometer stops feeding
/// fusion until the device is recalibrated.
#[tauri::command]
#[specta::specta]
pub async fn clear_mag_calibration(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<(), IpcError> {
    handle.db.set_setting(&mag_cal_key(mac), "")?;
    handle.state.set_mag_calibration(mac, None).await;
    tracing::info!(mac = ?mac, "mag calibration cleared");
    Ok(())
}

/// List the configured UDP haptic targets. The MAC field is a synthesized
/// locally-administered identifier — useful for binding OSC rules — and
/// not the receiver's real network MAC.
#[tauri::command]
#[specta::specta]
pub async fn udp_haptic_list(
    handle: State<'_, AppHandle>,
) -> Result<Vec<crate::udp_haptic::UdpHapticTarget>, IpcError> {
    let json = handle
        .db
        .get_setting(crate::udp_haptic::save_settings_key())?
        .unwrap_or_default();
    Ok(crate::udp_haptic::load_from_settings_json(&json))
}

/// Add or update a UDP haptic target. The synthesized MAC is derived
/// deterministically from `host:port`, so re-adding the same endpoint
/// returns the original MAC and keeps existing OSC bindings intact.
#[tauri::command]
#[specta::specta]
pub async fn udp_haptic_upsert(
    handle: State<'_, AppHandle>,
    alias: String,
    host: String,
    port: u16,
) -> Result<crate::udp_haptic::UdpHapticTarget, IpcError> {
    let mac = crate::udp_haptic::synth_mac(&host, port);
    let target = crate::udp_haptic::UdpHapticTarget {
        mac,
        alias,
        host,
        port,
    };
    let key = crate::udp_haptic::save_settings_key();
    let mut list = crate::udp_haptic::load_from_settings_json(
        &handle.db.get_setting(key)?.unwrap_or_default(),
    );
    if let Some(slot) = list.iter_mut().find(|t| t.mac == mac) {
        *slot = target.clone();
    } else {
        list.push(target.clone());
    }
    handle.db.set_setting(key, &serde_json::to_string(&list).unwrap_or_default())?;
    Ok(target)
}

/// Delete a UDP haptic target by its synthesized MAC.
#[tauri::command]
#[specta::specta]
pub async fn udp_haptic_remove(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
) -> Result<(), IpcError> {
    let key = crate::udp_haptic::save_settings_key();
    let mut list = crate::udp_haptic::load_from_settings_json(
        &handle.db.get_setting(key)?.unwrap_or_default(),
    );
    list.retain(|t| t.mac != mac);
    handle.db.set_setting(key, &serde_json::to_string(&list).unwrap_or_default())?;
    Ok(())
}

/// Fire a one-shot test pulse at a UDP haptic target. Intended for
/// confirming the receiver firmware is wired correctly before binding
/// the target into an OSC rule.
#[tauri::command]
#[specta::specta]
pub async fn udp_haptic_test(
    handle: State<'_, AppHandle>,
    mac: [u8; 6],
    intensity: f32,
    duration_ms: u16,
) -> Result<(), IpcError> {
    let key = crate::udp_haptic::save_settings_key();
    let list = crate::udp_haptic::load_from_settings_json(
        &handle.db.get_setting(key)?.unwrap_or_default(),
    );
    let target = list
        .into_iter()
        .find(|t| t.mac == mac)
        .ok_or(IpcError::NotFound)?;
    crate::udp_haptic::send(&target, intensity, duration_ms)
        .map_err(|e| IpcError::Internal(e.to_string()))
}

/// Write the embedded udev ruleset to `/etc/udev/rules.d/` via pkexec so
/// non-root Linux users can open the Joy-Con / DualSense / PSMove HID
/// nodes. Returns a no-op error on Windows and macOS — the UI uses that
/// to flip the button into a "Linux only" hint.
#[tauri::command]
#[specta::specta]
pub async fn install_udev_rules() -> Result<String, IpcError> {
    crate::udev_install::install().map_err(|e| IpcError::Internal(e.to_string()))
}

/// Look up the latest GitHub release and report whether an update is
/// available. Returns the running version even when up-to-date so the UI
/// can show "you're on the latest build".
#[tauri::command]
#[specta::specta]
pub async fn check_for_update() -> Result<crate::updater::UpdateInfo, IpcError> {
    crate::updater::check()
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

/// Download and install the latest release. The Tauri binary is replaced
/// in place; the UI should prompt the user to restart afterwards.
#[tauri::command]
#[specta::specta]
pub async fn apply_update() -> Result<crate::updater::UpdateInfo, IpcError> {
    crate::updater::apply()
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

/// Inspect Steam's controller_blacklist to detect when Steam Input is
/// grabbing Joy-Con / Switch Pro HID devices. The UI uses this to show a
/// warning banner with a 1-click fix.
#[tauri::command]
#[specta::specta]
pub async fn steam_blacklist_check() -> Result<crate::steam_blacklist::SteamBlacklistStatus, IpcError>
{
    Ok(crate::steam_blacklist::check())
}

/// Append the Joy-Con + Switch Pro VID/PID pairs to Steam's
/// controller_blacklist and persist the patched config.vdf. Steam must be
/// restarted for the change to take effect — surface that in the UI toast.
#[tauri::command]
#[specta::specta]
pub async fn steam_blacklist_apply_fix() -> Result<(), IpcError> {
    crate::steam_blacklist::apply_fix().map_err(|e| IpcError::Internal(e.to_string()))
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
