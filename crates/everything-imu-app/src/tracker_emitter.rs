//! Periodic emitters: TrackerUpdate / ImuSampleUpdate / BiasUpdate / ConnectionStatusUpdate.
//!
//! All four read from `watch::Receiver` snapshots stored in `AppState`,
//! so producers (the per-device pipeline tasks) never block on the IPC
//! bridge. Emit cadence is set per concern — IMU samples and rotation
//! batched at 30 Hz, bias and connection status at 1 Hz.

use crate::events::{
    BiasEntry, BiasUpdate, ConnectionStatusUpdate, ImuSampleEntry, ImuSampleUpdate, LatencyEntry,
    LatencyUpdate, TrackerSnapshot, TrackerUpdate,
};
use crate::state::AppHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle as TauriAppHandle, Manager};
use tauri_specta::Event;

pub fn spawn(app: &TauriAppHandle) {
    spawn_tracker_and_samples(app);
    spawn_bias(app);
    spawn_latency(app);
    spawn_connection_status(app);
}

/// 30 Hz: TrackerUpdate (orientation) + ImuSampleUpdate (raw samples).
fn spawn_tracker_and_samples(app: &TauriAppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        loop {
            interval.tick().await;
            let handle = match app.try_state::<AppHandle>() {
                Some(h) => h,
                None => continue,
            };
            let metas = handle.state.device_metadata_snapshot().await;
            let quats = handle.state.latest_quat_snapshot().await;
            let samples = handle.state.latest_sample_snapshot().await;
            let rates = handle.state.latest_rate_snapshot().await;
            let batteries = handle.state.latest_battery_snapshot().await;

            let mut trackers = Vec::with_capacity(metas.len());
            let mut sample_entries = Vec::with_capacity(metas.len());
            for m in &metas {
                let q = quats
                    .get(&m.id)
                    .map(|q| q.0)
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                trackers.push(TrackerSnapshot {
                    mac: m.id.mac,
                    serial: m.id.serial.clone(),
                    quat_xyzw: q,
                    battery_fraction: batteries.get(&m.id).copied().unwrap_or(f32::NAN),
                    rate_hz: rates.get(&m.id).copied().unwrap_or(0.0),
                });
                if let Some(s) = samples.get(&m.id) {
                    sample_entries.push(ImuSampleEntry {
                        mac: m.id.mac,
                        gyr_xyz: s.gyr_xyz,
                        acc_xyz: s.acc_xyz,
                        mag_xyz: s.mag_xyz,
                        elapsed_ms: s.elapsed_ms,
                    });
                }
            }
            let low_battery = trackers
                .iter()
                .filter(|t| {
                    t.battery_fraction.is_finite()
                        && t.battery_fraction > 0.0
                        && t.battery_fraction < 0.15
                })
                .count();
            crate::tray::update_tray_tooltip(&app, trackers.len(), low_battery);
            let _ = TrackerUpdate { trackers }.emit(&app);
            if !sample_entries.is_empty() {
                let _ = ImuSampleUpdate {
                    samples: sample_entries,
                }
                .emit(&app);
            }
        }
    });
}

/// 1 Hz: live VQF gyro-bias estimate per device.
fn spawn_bias(app: &TauriAppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let handle = match app.try_state::<AppHandle>() {
                Some(h) => h,
                None => continue,
            };
            let metas = handle.state.device_metadata_snapshot().await;
            let biases = handle.state.latest_bias_snapshot().await;
            let mut entries = Vec::with_capacity(metas.len());
            for m in &metas {
                if let Some(b) = biases.get(&m.id) {
                    entries.push(BiasEntry {
                        mac: m.id.mac,
                        gyr_bias: b.gyr_bias,
                    });
                }
            }
            if !entries.is_empty() {
                let _ = BiasUpdate { entries }.emit(&app);
            }
        }
    });
}

/// 1 Hz: bridge latency / jitter snapshot per device.
fn spawn_latency(app: &TauriAppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let handle = match app.try_state::<AppHandle>() {
                Some(h) => h,
                None => continue,
            };
            let metas = handle.state.device_metadata_snapshot().await;
            let lats = handle.state.latest_latency_snapshot().await;
            let mut entries = Vec::with_capacity(metas.len());
            for m in &metas {
                if let Some(snap) = lats.get(&m.id) {
                    if snap.samples_window == 0 {
                        continue;
                    }
                    entries.push(LatencyEntry {
                        mac: m.id.mac,
                        interval_us_p50: snap.interval_us_p50,
                        interval_us_p95: snap.interval_us_p95,
                        interval_us_p99: snap.interval_us_p99,
                        jitter_us: snap.jitter_us,
                        send_us_p50: snap.send_us_p50,
                        send_us_p95: snap.send_us_p95,
                        dropped_estimate: snap.dropped_estimate,
                        samples_window: snap.samples_window,
                    });
                }
            }
            if !entries.is_empty() {
                let _ = LatencyUpdate { entries }.emit(&app);
            }
        }
    });
}

/// 1 Hz: SlimeClient stats snapshot.
fn spawn_connection_status(app: &TauriAppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let handle = match app.try_state::<AppHandle>() {
                Some(h) => h,
                None => continue,
            };
            let stats = handle.state.aggregated_stats().await;
            let server_addr = handle
                .db
                .get_setting("slime_server_addr")
                .ok()
                .flatten()
                .unwrap_or_else(|| "127.0.0.1:6969".to_string());
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
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
            let _ = ConnectionStatusUpdate {
                server_addr,
                server_supports_bundle: stats.server_supports_bundle,
                packets_sent: stats.packets_sent,
                last_send_ms_ago,
                last_handshake_ms_ago,
            }
            .emit(&app);
        }
    });
}
