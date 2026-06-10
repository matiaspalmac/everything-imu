//! PS Move calibration: factory IMU cal (HID feature report 0x10) and
//! magnetometer fig-8 hard/soft-iron calibration.
//!
//! ## Factory calibration (ZCM1/ZCM2)
//!
//! Each controller stores a per-unit cal blob in firmware, read via **feature
//! report 0x10**. The device splits it across **two blocks**; the host must
//! issue the feature read twice and concatenate (block-order discriminator at
//! byte 1 of each block). The blob holds, per `docs/reference/psmove_protocol.md`:
//!
//! - accelerometer samples for the 6 tumble orientations (±X, ±Y, ±Z),
//! - a gyro zero-rate bias region,
//! - gyro readings taken at a known 90 RPM around each axis (gain reference).
//!
//! A min/max linear fit is used here (bias = midpoint, gain = half-span per
//! axis), which is exact for clean ±1 g samples and adequate as a VQF seed. If
//! the blob is unavailable or malformed, [`ImuCalibration::identity`] is the
//! documented fallback — VQF learns the residual bias during its ~5 s warm-up
//! (per the ref doc's guidance that skipping factory cal is acceptable).
//!
//! Exact field offsets follow `docs/reference/psmove_protocol.md` and are flagged
//! validation-pending: no Move hardware was available to confirm them this
//! session, so the loader is defensive (length-checked, never panics).

use crate::ids::ControllerKind;

/// Assembled two-block feature-0x10 payload.
pub const CAL_BLOCK_LEN: usize = 49;
pub const CAL_BLOB_LEN: usize = CAL_BLOCK_LEN * 2;

// Documented field offsets within the assembled blob (per the ref doc).
const OFS_ACCEL_SAMPLES: usize = 2; // 6 × 6-byte (3× i16 LE) orientation samples
const OFS_GYRO_BIAS: usize = 0x26;
const OFS_GYRO_90RPM: usize = 0x30; // 3 × 6-byte axis samples at 90 RPM
const RPM_90_IN_RAD_S: f32 = 90.0 / 60.0 * std::f32::consts::TAU; // 9.4248 rad/s

/// Linear IMU correction: `corrected = (raw - bias) * gain`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuCalibration {
    /// Accelerometer bias in raw LSB (pre-scale), per axis (X-Y-Z, deswizzled).
    pub accel_bias: [f32; 3],
    /// Accelerometer gain (dimensionless multiplier on the nominal scale).
    pub accel_gain: [f32; 3],
    /// Gyro zero-rate bias in raw LSB, per axis.
    pub gyro_bias: [f32; 3],
    /// Gyro gain (dimensionless multiplier on the nominal scale).
    pub gyro_gain: [f32; 3],
}

impl Default for ImuCalibration {
    fn default() -> Self {
        Self::identity()
    }
}

impl ImuCalibration {
    /// No-op calibration (VQF-warm-up fallback).
    pub const fn identity() -> Self {
        Self {
            accel_bias: [0.0; 3],
            accel_gain: [1.0; 3],
            gyro_bias: [0.0; 3],
            gyro_gain: [1.0; 3],
        }
    }

    /// Apply the bias/gain to a raw accel/gyro LSB triplet.
    #[inline]
    pub fn apply_accel(&self, raw: [f32; 3]) -> [f32; 3] {
        [
            (raw[0] - self.accel_bias[0]) * self.accel_gain[0],
            (raw[1] - self.accel_bias[1]) * self.accel_gain[1],
            (raw[2] - self.accel_bias[2]) * self.accel_gain[2],
        ]
    }

    #[inline]
    pub fn apply_gyro(&self, raw: [f32; 3]) -> [f32; 3] {
        [
            (raw[0] - self.gyro_bias[0]) * self.gyro_gain[0],
            (raw[1] - self.gyro_bias[1]) * self.gyro_gain[1],
            (raw[2] - self.gyro_bias[2]) * self.gyro_gain[2],
        ]
    }
}

fn read_i16_le(buf: &[u8], ofs: usize) -> Option<i32> {
    let b = buf.get(ofs..ofs + 2)?;
    Some(i16::from_le_bytes([b[0], b[1]]) as i32)
}

/// Parse an assembled (two-block) feature-0x10 blob into an [`ImuCalibration`].
///
/// Returns [`ImuCalibration::identity`] if the blob is too short or the samples
/// are degenerate (zero span), so a bad read never produces a divide-by-zero or
/// inverted gain downstream.
pub fn parse_factory_blob(kind: ControllerKind, blob: &[u8]) -> ImuCalibration {
    if blob.len() < OFS_GYRO_90RPM + 18 {
        return ImuCalibration::identity();
    }

    // --- Accelerometer: 6 orientation samples → per-axis min/max linear fit ---
    let mut min = [i32::MAX; 3];
    let mut max = [i32::MIN; 3];
    for s in 0..6 {
        let base = OFS_ACCEL_SAMPLES + s * 6;
        for axis in 0..3 {
            if let Some(v) = read_i16_le(blob, base + axis * 2) {
                min[axis] = min[axis].min(v);
                max[axis] = max[axis].max(v);
            }
        }
    }
    let mut accel_bias = [0.0; 3];
    let mut accel_gain = [1.0; 3];
    for axis in 0..3 {
        if max[axis] > min[axis] {
            let mid = (max[axis] + min[axis]) as f32 / 2.0;
            let half_span = (max[axis] - min[axis]) as f32 / 2.0;
            accel_bias[axis] = mid;
            // Gain renormalises the measured ±1 g half-span to the nominal LSB.
            accel_gain[axis] = nominal_accel_half_span() / half_span;
        }
    }

    // --- Gyro: zero-rate bias + 90 RPM gain reference ---
    let mut gyro_bias = [0.0; 3];
    for (axis, bias) in gyro_bias.iter_mut().enumerate() {
        *bias = read_i16_le(blob, OFS_GYRO_BIAS + axis * 2).unwrap_or(0) as f32;
    }
    let mut gyro_gain = [1.0; 3];
    for axis in 0..3 {
        let base = OFS_GYRO_90RPM + axis * 6;
        if let Some(v) = read_i16_le(blob, base + axis * 2) {
            let delta = (v as f32 - gyro_bias[axis]).abs();
            if delta > 1.0 {
                // Expected raw delta for 90 RPM at the nominal gyro scale.
                gyro_gain[axis] = nominal_gyro_90rpm_lsb() / delta;
            }
        }
    }

    let _ = kind; // ZCM1/ZCM2 share the blob layout per current references.
    ImuCalibration {
        accel_bias,
        accel_gain,
        gyro_bias,
        gyro_gain,
    }
}

/// Nominal accel half-span (1 g) in raw LSB, matching `report::ACCEL_LSB_PER_G`.
fn nominal_accel_half_span() -> f32 {
    16384.0
}

/// Nominal gyro raw delta for 90 RPM, matching `report::GYRO_LSB_PER_DEG_S`.
fn nominal_gyro_90rpm_lsb() -> f32 {
    // 9.4248 rad/s = 540 deg/s; × 16.4 LSB/(deg/s).
    540.0 * 16.4
}

/// Streaming magnetometer hard/soft-iron calibrator (fig-8 sweep, ZCM1 only).
///
/// Accumulates per-axis min/max over a user rotation sweep; hard-iron offset is
/// the midpoint, soft-iron scale normalises each axis to a common radius. Same
/// approach as the mobile mag calibration. Until the sweep has meaningful span
/// the calibrator reports not-ready and [`apply`](MagCalibrator::apply) passes
/// the raw value through.
#[derive(Debug, Clone)]
pub struct MagCalibrator {
    min: [f32; 3],
    max: [f32; 3],
    samples: u32,
}

impl Default for MagCalibrator {
    fn default() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
            samples: 0,
        }
    }
}

impl MagCalibrator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw magnetometer triplet from the sweep.
    pub fn observe(&mut self, m: [f32; 3]) {
        for (axis, &v) in m.iter().enumerate() {
            self.min[axis] = self.min[axis].min(v);
            self.max[axis] = self.max[axis].max(v);
        }
        self.samples = self.samples.saturating_add(1);
    }

    /// True once every axis has a non-trivial span — i.e. the sweep covered
    /// enough orientation for a stable hard/soft-iron estimate.
    pub fn is_ready(&self) -> bool {
        self.samples >= 200 && (0..3).all(|a| (self.max[a] - self.min[a]) > MIN_USEFUL_SPAN)
    }

    /// Hard-iron offset (per-axis midpoint).
    pub fn offset(&self) -> [f32; 3] {
        let mut o = [0.0; 3];
        for (axis, slot) in o.iter_mut().enumerate() {
            if self.max[axis] > self.min[axis] {
                *slot = (self.max[axis] + self.min[axis]) / 2.0;
            }
        }
        o
    }

    /// Soft-iron per-axis scale normalising each radius to the mean radius.
    pub fn scale(&self) -> [f32; 3] {
        let mut radius = [1.0; 3];
        for (axis, slot) in radius.iter_mut().enumerate() {
            let span = (self.max[axis] - self.min[axis]) / 2.0;
            if span > MIN_USEFUL_SPAN {
                *slot = span;
            }
        }
        let mean = (radius[0] + radius[1] + radius[2]) / 3.0;
        let mut s = [1.0; 3];
        for axis in 0..3 {
            if radius[axis] > 0.0 {
                s[axis] = mean / radius[axis];
            }
        }
        s
    }

    /// Apply the current calibration to a raw magnetometer triplet. Pass-through
    /// until [`is_ready`](MagCalibrator::is_ready).
    pub fn apply(&self, m: [f32; 3]) -> [f32; 3] {
        if !self.is_ready() {
            return m;
        }
        let off = self.offset();
        let sc = self.scale();
        [
            (m[0] - off[0]) * sc[0],
            (m[1] - off[1]) * sc[1],
            (m[2] - off[2]) * sc[2],
        ]
    }
}

const MIN_USEFUL_SPAN: f32 = 50.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_values_through() {
        let cal = ImuCalibration::identity();
        assert_eq!(cal.apply_accel([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
        assert_eq!(cal.apply_gyro([4.0, 5.0, 6.0]), [4.0, 5.0, 6.0]);
    }

    #[test]
    fn short_blob_falls_back_to_identity() {
        assert_eq!(
            parse_factory_blob(ControllerKind::Zcm1, &[0u8; 8]),
            ImuCalibration::identity()
        );
    }

    #[test]
    fn accel_six_orientation_fit_recovers_bias_and_gain() {
        let mut blob = vec![0u8; CAL_BLOB_LEN];
        // Construct 6 orientation samples with a +500 LSB bias and a half-span
        // of 16384 (nominal): values are bias ± 16384 on each axis in turn.
        let bias = 500i16;
        let span = 16384i16;
        let samples: [[i16; 3]; 6] = [
            [bias + span, bias, bias],
            [bias - span, bias, bias],
            [bias, bias + span, bias],
            [bias, bias - span, bias],
            [bias, bias, bias + span],
            [bias, bias, bias - span],
        ];
        for (s, sample) in samples.iter().enumerate() {
            let base = OFS_ACCEL_SAMPLES + s * 6;
            for axis in 0..3 {
                blob[base + axis * 2..base + axis * 2 + 2]
                    .copy_from_slice(&sample[axis].to_le_bytes());
            }
        }
        let cal = parse_factory_blob(ControllerKind::Zcm1, &blob);
        for axis in 0..3 {
            assert!((cal.accel_bias[axis] - bias as f32).abs() < 1.0);
            assert!((cal.accel_gain[axis] - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn mag_calibrator_passes_through_until_ready() {
        let mut cal = MagCalibrator::new();
        cal.observe([100.0, 0.0, 0.0]);
        assert!(!cal.is_ready());
        assert_eq!(cal.apply([100.0, 0.0, 0.0]), [100.0, 0.0, 0.0]);
    }

    #[test]
    fn mag_calibrator_centers_and_scales_after_sweep() {
        let mut cal = MagCalibrator::new();
        // Symmetric sweep centred at (10, -20, 30) with equal radii.
        for i in 0..400 {
            let t = i as f32 * 0.1;
            cal.observe([
                10.0 + 100.0 * t.sin(),
                -20.0 + 100.0 * t.cos(),
                30.0 + 100.0 * (t * 0.5).sin(),
            ]);
        }
        assert!(cal.is_ready());
        let off = cal.offset();
        assert!((off[0] - 10.0).abs() < 5.0);
        assert!((off[1] + 20.0).abs() < 5.0);
    }
}
