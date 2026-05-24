//! Magnetometer hard-iron calibration — algebraic sphere fit.
//!
//! A magnetometer worn on the body reads a field offset by nearby ferrous
//! material and DC currents (hard-iron distortion). Uncorrected, the field
//! vector traces a sphere that is *not* centred on the origin, and yaw derived
//! from it is wrong. Rotating the device through all orientations samples that
//! sphere; fitting its centre yields the hard-iron offset to subtract.
//!
//! Soft-iron (ellipsoid) distortion is intentionally not modelled — the small
//! controller PCB contributes little, and a sphere fit is far more robust with
//! the sparse, hand-rotated sample sets a user can realistically produce.

use nalgebra::{Matrix4, Vector4};
use serde::{Deserialize, Serialize};

/// Result of an algebraic sphere fit over a magnetometer sample cloud.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SphereFit {
    /// Sphere centre — the hard-iron offset to subtract from raw mag samples.
    pub center: [f32; 3],
    /// Sphere radius — the corrected field magnitude (µT if input is µT).
    pub radius: f32,
    /// RMS of `||p - center|| - radius` over the input. Lower is a tighter fit.
    pub residual: f32,
}

/// Persisted hard-iron calibration for one device.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MagCalibration {
    /// Hard-iron offset (µT) subtracted from raw mag samples before fusion.
    pub offset: [f32; 3],
    /// Fitted field magnitude (µT). Earth's field is ~25-65 µT; a wildly
    /// different value hints at a bad fit or heavy soft-iron distortion.
    pub field_strength_ut: f32,
    /// Sphere-fit RMS residual (µT).
    pub residual: f32,
    /// Direction-bin coverage `0.0..=1.0` of the sample set used for the fit.
    pub coverage: f32,
}

/// Number of reachable direction bins on the 3×3×3 grid. The centre cell
/// `(1,1,1)` is unreachable by a unit vector, leaving 26.
const COVERAGE_BINS: usize = 26;

/// Minimum samples for a numerically meaningful 4-parameter fit.
const MIN_FIT_SAMPLES: usize = 12;

/// Fit a sphere to `points` by algebraic least squares.
///
/// Linearises `x² + y² + z² = 2a·x + 2b·y + 2c·z + d` (linear in the unknowns
/// `a, b, c, d`), accumulates the 4×4 normal equations, and solves them.
/// `center = (a, b, c)` and `radius = sqrt(d + a² + b² + c²)`.
///
/// Returns `None` for fewer than [`MIN_FIT_SAMPLES`] points, a singular normal
/// matrix (e.g. coplanar samples), or a non-positive radius.
pub fn fit_sphere(points: &[[f32; 3]]) -> Option<SphereFit> {
    if points.len() < MIN_FIT_SAMPLES {
        return None;
    }
    // Normal equations AᵀA · params = Aᵀf, with row_i = [2x, 2y, 2z, 1]
    // and f_i = x² + y² + z². Accumulated in f64 for conditioning.
    let mut ata = Matrix4::<f64>::zeros();
    let mut atf = Vector4::<f64>::zeros();
    for p in points {
        let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
        let row = [2.0 * x, 2.0 * y, 2.0 * z, 1.0];
        let f = x * x + y * y + z * z;
        for i in 0..4 {
            for j in 0..4 {
                ata[(i, j)] += row[i] * row[j];
            }
            atf[i] += row[i] * f;
        }
    }
    let params = ata.lu().solve(&atf)?;
    let (a, b, c, d) = (params[0], params[1], params[2], params[3]);
    let r_sq = d + a * a + b * b + c * c;
    if !(r_sq.is_finite() && r_sq > 0.0) {
        return None;
    }
    let radius = r_sq.sqrt();
    let center = [a as f32, b as f32, c as f32];

    // RMS residual of the fit.
    let mut sum_sq = 0.0_f64;
    for p in points {
        let dx = p[0] as f64 - a;
        let dy = p[1] as f64 - b;
        let dz = p[2] as f64 - c;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        sum_sq += (dist - radius) * (dist - radius);
    }
    let residual = (sum_sq / points.len() as f64).sqrt();

    Some(SphereFit {
        center,
        radius: radius as f32,
        residual: residual as f32,
    })
}

/// Map a unit-ish direction component to a bin index `0..=2`.
fn axis_bin(v: f32) -> usize {
    if v < -1.0 / 3.0 {
        0
    } else if v < 1.0 / 3.0 {
        1
    } else {
        2
    }
}

/// Fraction `0.0..=1.0` of the 26 direction bins touched by `points` once
/// re-centred on `center`. A figure-8 rotation through all orientations
/// approaches 1.0; a device barely moved stays near 0.
///
/// Each `(p - center)` is normalised and its three components binned into a
/// 3×3×3 grid. The unreachable centre cell is excluded.
pub fn coverage(points: &[[f32; 3]], center: [f32; 3]) -> f32 {
    let mut seen = [false; 27];
    for p in points {
        let d = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
        let norm = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if norm < f32::EPSILON {
            continue;
        }
        let u = [d[0] / norm, d[1] / norm, d[2] / norm];
        let cell = axis_bin(u[0]) * 9 + axis_bin(u[1]) * 3 + axis_bin(u[2]);
        seen[cell] = true;
    }
    // Cell 13 == (1,1,1), the unreachable centre — never count it.
    let touched = seen
        .iter()
        .enumerate()
        .filter(|(i, &s)| *i != 13 && s)
        .count();
    touched as f32 / COVERAGE_BINS as f32
}

/// Fit a [`MagCalibration`] from a raw magnetometer sample cloud (µT).
///
/// Returns `None` when [`fit_sphere`] fails. The caller decides whether the
/// resulting `coverage` / `residual` are good enough to accept.
pub fn calibrate(points: &[[f32; 3]]) -> Option<MagCalibration> {
    let fit = fit_sphere(points)?;
    Some(MagCalibration {
        offset: fit.center,
        field_strength_ut: fit.radius,
        residual: fit.residual,
        coverage: coverage(points, fit.center),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate points on a sphere of `radius` centred at `center`, walking a
    /// lattice of spherical angles for deterministic, well-spread coverage.
    fn sphere_points(center: [f32; 3], radius: f32, steps: usize) -> Vec<[f32; 3]> {
        let mut pts = Vec::new();
        for i in 0..steps {
            let theta = std::f32::consts::PI * (i as f32 + 0.5) / steps as f32;
            for j in 0..steps {
                let phi = 2.0 * std::f32::consts::PI * j as f32 / steps as f32;
                pts.push([
                    center[0] + radius * theta.sin() * phi.cos(),
                    center[1] + radius * theta.sin() * phi.sin(),
                    center[2] + radius * theta.cos(),
                ]);
            }
        }
        pts
    }

    #[test]
    fn recovers_offset_sphere_center() {
        let center = [12.0, -30.0, 7.5];
        let pts = sphere_points(center, 48.0, 12);
        let fit = fit_sphere(&pts).expect("fit");
        for (k, (got, want)) in fit.center.iter().zip(center.iter()).enumerate() {
            assert!((got - want).abs() < 1e-2, "axis {k}: got {got} want {want}",);
        }
        assert!((fit.radius - 48.0).abs() < 1e-2, "radius {}", fit.radius);
        assert!(fit.residual < 1e-2, "residual {}", fit.residual);
    }

    #[test]
    fn full_coverage_for_full_sphere() {
        let center = [1.0, 2.0, 3.0];
        let pts = sphere_points(center, 50.0, 12);
        let cov = coverage(&pts, center);
        assert!(cov > 0.99, "coverage {cov}");
    }

    #[test]
    fn partial_coverage_for_one_octant() {
        let center = [0.0, 0.0, 0.0];
        // Only the +,+,+ octant.
        let pts = sphere_points([0.0, 0.0, 0.0], 50.0, 16)
            .into_iter()
            .filter(|p| p[0] > 5.0 && p[1] > 5.0 && p[2] > 5.0)
            .collect::<Vec<_>>();
        let cov = coverage(&pts, center);
        assert!(cov > 0.0 && cov < 0.5, "coverage {cov}");
    }

    #[test]
    fn rejects_too_few_points() {
        let pts = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(fit_sphere(&pts).is_none());
    }

    #[test]
    fn rejects_coplanar_points() {
        // All z == 0 → singular normal matrix.
        let pts: Vec<[f32; 3]> = (0..40)
            .map(|i| {
                let a = i as f32 * 0.3;
                [a.cos() * 20.0, a.sin() * 20.0, 0.0]
            })
            .collect();
        assert!(fit_sphere(&pts).is_none());
    }

    #[test]
    fn calibrate_round_trips_offset() {
        let center = [-8.0, 14.0, 22.0];
        let pts = sphere_points(center, 45.0, 12);
        let cal = calibrate(&pts).expect("calibrate");
        for (got, want) in cal.offset.iter().zip(center.iter()) {
            assert!((got - want).abs() < 1e-2);
        }
        assert!(cal.coverage > 0.99);
    }

    #[test]
    fn mag_calibration_json_round_trip() {
        let cal = MagCalibration {
            offset: [1.5, -2.5, 3.5],
            field_strength_ut: 47.0,
            residual: 0.8,
            coverage: 0.92,
        };
        let json = serde_json::to_string(&cal).unwrap();
        let back: MagCalibration = serde_json::from_str(&json).unwrap();
        assert_eq!(cal, back);
    }
}
