//! Synthesise IMU samples from a Tesla streaming frame.
//!
//! The car gives us heading (degrees, world frame, clockwise from north) and
//! speed (mph). From the first-derivative we synthesise:
//!
//! - **Gyro Z (yaw rate)**: `(heading_t - heading_{t-1}) / dt`, unwrapped at
//!   the 0/360 boundary so a turn through north doesn't spike the rate.
//! - **Accel X (longitudinal)**: `(speed_t - speed_{t-1}) / dt`.
//! - **Accel Y (lateral / centripetal)**: `speed * yaw_rate` — straightforward
//!   uniform-circular-motion derivation.
//! - **Accel Z**: world gravity, `+9.81 m/s²` in the body frame because the
//!   Tesla sits flat. (Tilt while accelerating is fractions of a degree;
//!   we ignore it.)
//!
//! Gyro X/Y are clamped to zero — the car doesn't roll or pitch in any way
//! the IMU pipeline cares about.
//!
//! All output units are in the everything-imu convention:
//! gyro = rad/s, accel = m/s², all body-frame.

use device_traits::ImuSample;

/// 1 mph ≈ 0.44704 m/s.
const MPH_TO_MPS: f32 = 0.44704;
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;
const GRAVITY_MPS2: f32 = 9.806_65;

/// Rolling state required to take first-derivatives between streaming frames.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImuSynth {
    last_heading_deg: Option<f32>,
    last_speed_mps: Option<f32>,
    last_timestamp_us: Option<u64>,
}

impl ImuSynth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset internal state — call when the streaming socket reconnects so
    /// the first post-reconnect frame doesn't synthesize a huge spike.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feed one streaming frame. Returns `None` for the very first frame
    /// (no prior sample to differentiate against) and for frames where the
    /// required fields are missing.
    pub fn ingest(
        &mut self,
        timestamp_us: u64,
        heading_deg: Option<f32>,
        speed_mph: Option<f32>,
    ) -> Option<ImuSample> {
        let heading = heading_deg?;
        let speed_mps = speed_mph? * MPH_TO_MPS;
        let prev_ts = self.last_timestamp_us.replace(timestamp_us);
        let prev_heading = self.last_heading_deg.replace(heading);
        let prev_speed = self.last_speed_mps.replace(speed_mps);

        let (prev_ts, prev_heading, prev_speed) =
            match (prev_ts, prev_heading, prev_speed) {
                (Some(t), Some(h), Some(s)) => (t, h, s),
                _ => return None,
            };

        let dt_s = (timestamp_us.saturating_sub(prev_ts)) as f32 / 1_000_000.0;
        if !(dt_s.is_finite() && dt_s > 1e-3) {
            // Frames closer than 1 ms apart almost certainly came out-of-order
            // or duplicated. Skip rather than dividing by ~0.
            return None;
        }

        let yaw_rate_rad_s = unwrapped_delta_deg(prev_heading, heading) * DEG_TO_RAD / dt_s;
        let longitudinal_accel = (speed_mps - prev_speed) / dt_s;
        let lateral_accel = speed_mps * yaw_rate_rad_s;

        Some(ImuSample {
            gyro: [0.0, 0.0, yaw_rate_rad_s],
            accel: [longitudinal_accel, lateral_accel, GRAVITY_MPS2],
            mag: None,
            timestamp_us,
        })
    }
}

/// Return `b - a` in degrees, mapped to the shortest signed angular distance
/// in `(-180, 180]`. Needed so heading transitions through the 0/360
/// boundary don't spike the synthesised gyro.
pub fn unwrapped_delta_deg(a: f32, b: f32) -> f32 {
    let mut d = b - a;
    while d > 180.0 {
        d -= 360.0;
    }
    while d <= -180.0 {
        d += 360.0;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn delta_through_north_is_shortest_path() {
        // 350° → 10° should be +20°, not −340°.
        assert_relative_eq!(unwrapped_delta_deg(350.0, 10.0), 20.0, epsilon = 1e-4);
        // 10° → 350° should be −20°, not +340°.
        assert_relative_eq!(unwrapped_delta_deg(10.0, 350.0), -20.0, epsilon = 1e-4);
    }

    #[test]
    fn delta_at_180_uses_negative_side() {
        // -180 boundary: opposite headings map to the +180 representative.
        let d = unwrapped_delta_deg(0.0, 180.0);
        assert!(d.abs() <= 180.0);
        assert_relative_eq!(d, 180.0, epsilon = 1e-4);
    }

    #[test]
    fn first_frame_emits_nothing() {
        let mut s = ImuSynth::new();
        let sample = s.ingest(0, Some(90.0), Some(30.0));
        assert!(sample.is_none(), "first frame establishes baseline only");
    }

    #[test]
    fn straight_line_acceleration_recovers_longitudinal_accel() {
        // Heading constant, speed grows by 10 mph (~4.47 m/s) over 1 s.
        let mut s = ImuSynth::new();
        let _ = s.ingest(0, Some(90.0), Some(20.0));
        let sample = s
            .ingest(1_000_000, Some(90.0), Some(30.0))
            .expect("synth");
        assert_relative_eq!(sample.gyro[2], 0.0, epsilon = 1e-4);
        assert_relative_eq!(
            sample.accel[0],
            10.0 * MPH_TO_MPS,
            epsilon = 1e-3,
        );
        // No turn → no centripetal accel.
        assert_relative_eq!(sample.accel[1], 0.0, epsilon = 1e-4);
        // Gravity baseline always present on Z.
        assert_relative_eq!(sample.accel[2], GRAVITY_MPS2, epsilon = 1e-4);
    }

    #[test]
    fn steady_circle_recovers_yaw_rate_and_centripetal() {
        // Constant 30 mph (~13.41 m/s), heading sweeping 90°/s.
        let mut s = ImuSynth::new();
        let _ = s.ingest(0, Some(0.0), Some(30.0));
        let sample = s
            .ingest(1_000_000, Some(90.0), Some(30.0))
            .expect("synth");
        assert_relative_eq!(
            sample.gyro[2],
            90.0 * DEG_TO_RAD,
            epsilon = 1e-3,
        );
        // Speed unchanged → no longitudinal accel.
        assert_relative_eq!(sample.accel[0], 0.0, epsilon = 1e-3);
        // Centripetal = v · ω. Sign matches yaw rate direction.
        let expected = 30.0 * MPH_TO_MPS * (90.0 * DEG_TO_RAD);
        assert_relative_eq!(sample.accel[1], expected, epsilon = 1e-3);
    }

    #[test]
    fn duplicate_timestamp_returns_none_no_division_by_zero() {
        let mut s = ImuSynth::new();
        let _ = s.ingest(100, Some(0.0), Some(10.0));
        // dt = 0 → skip.
        let sample = s.ingest(100, Some(1.0), Some(11.0));
        assert!(sample.is_none());
    }

    #[test]
    fn missing_fields_return_none() {
        let mut s = ImuSynth::new();
        let _ = s.ingest(0, Some(0.0), Some(10.0));
        assert!(s.ingest(1_000_000, None, Some(11.0)).is_none());
        assert!(s.ingest(1_000_000, Some(1.0), None).is_none());
    }

    #[test]
    fn reset_clears_state_no_spike_after_reconnect() {
        let mut s = ImuSynth::new();
        let _ = s.ingest(0, Some(0.0), Some(60.0));
        s.reset();
        // First post-reset frame should produce nothing — no spike from
        // re-deriving against the pre-reset 60 mph baseline.
        assert!(s.ingest(1_000_000, Some(90.0), Some(0.0)).is_none());
    }
}
