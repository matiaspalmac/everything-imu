//! Synthetic Tesla — replay a procedural drive without hitting the real API.
//!
//! Generates a figure-eight at constant ~30 mph that completes one full loop
//! every 60 s. Plenty of yaw/lateral-accel variation to exercise the IMU
//! synth path and fusion downstream.

use std::time::Duration;

use device_traits::{ChannelInfo, DeviceError, DeviceId, ImuSample};
use tokio::sync::mpsc;

use crate::api::StreamFrame;
use crate::imu::ImuSynth;

/// Drives a synthetic figure-eight and writes the resulting [`ChannelInfo`]
/// stream through `tx`. Runs until the receiver is dropped or `stop` fires.
pub async fn run_synthetic_loop(
    rate_hz: u16,
    device_id: DeviceId,
    tx: mpsc::Sender<ChannelInfo>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), DeviceError> {
    let rate_hz = rate_hz.max(1) as u64;
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / rate_hz));
    let mut synth = ImuSynth::new();
    let mut t_us: u64 = 0;
    let dt_us: u64 = 1_000_000 / rate_hz;

    let _ = tx.send(ChannelInfo::Connected(device_id.clone())).await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let frame = procedural_frame(t_us);
                if let Some(sample) = synth.ingest(
                    frame.timestamp_ms.saturating_mul(1000),
                    frame.heading_deg,
                    frame.speed_mph,
                ) {
                    if tx.send(ChannelInfo::ImuSamples(vec![sample])).await.is_err() {
                        return Ok(());
                    }
                }
                t_us = t_us.wrapping_add(dt_us);
            }
            _ = stop.changed() => {
                if *stop.borrow() {
                    let _ = tx.send(ChannelInfo::Disconnected).await;
                    return Ok(());
                }
            }
        }
    }
}

/// Procedurally generate a figure-eight frame at `t_us` microseconds into
/// the replay. One full loop every 60 s; speed clamped to 30 mph.
pub fn procedural_frame(t_us: u64) -> StreamFrame {
    let t_s = t_us as f32 / 1_000_000.0;
    let loop_s = 60.0_f32;
    let phase = (t_s % loop_s) / loop_s; // 0..1
    let theta = phase * std::f32::consts::TAU;
    // Figure-eight (lemniscate parameterisation) — heading is the tangent
    // angle, projected into 0..360 to mimic a compass.
    let dx = theta.cos();
    let dy = (2.0 * theta).cos() * 0.5;
    let heading_rad = dy.atan2(dx);
    let heading_deg = (heading_rad.to_degrees() + 360.0) % 360.0;
    StreamFrame {
        timestamp_ms: t_us / 1000,
        speed_mph: Some(30.0),
        heading_deg: Some(heading_deg),
        power_kw: Some(10.0),
        shift_state: Some("D".into()),
        est_lat: None,
        est_lng: None,
        est_heading_deg: Some(heading_deg),
    }
}

/// Convenience for tests — synthesises N samples from the procedural feed
/// without touching the network or a tokio runtime.
pub fn collect_samples(n: usize, rate_hz: u16) -> Vec<ImuSample> {
    let rate_hz = rate_hz.max(1) as u64;
    let dt_us: u64 = 1_000_000 / rate_hz;
    let mut synth = ImuSynth::new();
    let mut out = Vec::new();
    for i in 0..n {
        let t_us = (i as u64) * dt_us;
        let frame = procedural_frame(t_us);
        if let Some(s) = synth.ingest(t_us, frame.heading_deg, frame.speed_mph) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_heading_within_compass_range() {
        for i in 0..1000 {
            let t_us = (i as u64) * 100_000;
            let frame = procedural_frame(t_us);
            let h = frame.heading_deg.unwrap();
            assert!(h.is_finite());
            assert!((0.0..360.0).contains(&h), "heading {h} outside compass range");
        }
    }

    #[test]
    fn collect_samples_yields_non_zero_yaw_rate() {
        let samples = collect_samples(120, 10);
        assert!(!samples.is_empty());
        // At least one sample must have a non-zero yaw rate; otherwise the
        // procedural drive is straight-line and useless for fusion tests.
        let any_yaw = samples.iter().any(|s| s.gyro[2].abs() > 1e-3);
        assert!(any_yaw, "procedural drive must produce yaw rate variation");
    }

    #[test]
    fn collect_samples_gravity_always_positive_z() {
        let samples = collect_samples(60, 10);
        for s in &samples {
            assert!(s.accel[2] > 9.0 && s.accel[2] < 10.0);
        }
    }
}
