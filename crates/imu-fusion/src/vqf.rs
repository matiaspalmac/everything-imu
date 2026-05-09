//! VQF Implementation.
//! f64 internal precision.
//!
//! Based on theory from:
//!   Laidig & Seel, "VQF: Highly Accurate IMU Orientation Estimation with Bias Estimation
//!   and Magnetic Disturbance Rejection." Information Fusion 91 (2023) 187-204.
//!   <https://arxiv.org/abs/2203.17024>

use nalgebra::{Matrix3, Vector3 as NaVec3};
use std::f64::consts::PI;

const EPS: f64 = 1e-6;

/// VQF tunable parameters. All angle units are DEGREES at the boundary, converted
/// to radians internally during `setup()`.
#[derive(Clone, Debug)]
pub struct VqfParams {
    pub tau_acc: f64,
    pub tau_mag: f64,
    pub motion_bias_est_enabled: bool,
    pub rest_bias_est_enabled: bool,
    pub mag_dist_rejection_enabled: bool,
    pub bias_sigma_init: f64,
    pub bias_forgetting_time: f64,
    pub bias_clip: f64,
    pub bias_sigma_motion: f64,
    pub bias_vertical_forgetting_factor: f64,
    pub bias_sigma_rest: f64,
    pub rest_min_t: f64,
    pub rest_filter_tau: f64,
    pub rest_th_gyr: f64,
    pub rest_th_acc: f64,
    pub mag_current_tau: f64,
    pub mag_ref_tau: f64,
    pub mag_norm_th: f64,
    pub mag_dip_th: f64,
    pub mag_new_time: f64,
    pub mag_new_first_time: f64,
    pub mag_new_min_gyr: f64,
    pub mag_min_undisturbed_time: f64,
    pub mag_max_rejection_time: f64,
    pub mag_rejection_factor: f64,
}

impl Default for VqfParams {
    fn default() -> Self {
        Self {
            tau_acc: 3.0,
            tau_mag: 9.0,
            motion_bias_est_enabled: true,
            rest_bias_est_enabled: true,
            mag_dist_rejection_enabled: true,
            bias_sigma_init: 0.5,
            bias_forgetting_time: 100.0,
            bias_clip: 2.0,
            bias_sigma_motion: 0.1,
            bias_vertical_forgetting_factor: 1e-4,
            bias_sigma_rest: 0.03,
            rest_min_t: 1.5,
            rest_filter_tau: 0.5,
            rest_th_gyr: 2.0,
            rest_th_acc: 0.5,
            mag_current_tau: 0.05,
            mag_ref_tau: 20.0,
            mag_norm_th: 0.1,
            mag_dip_th: 10.0,
            mag_new_time: 20.0,
            mag_new_first_time: 5.0,
            mag_new_min_gyr: 20.0,
            mag_min_undisturbed_time: 0.5,
            mag_max_rejection_time: 60.0,
            mag_rejection_factor: 2.0,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VqfState {
    pub gyr_quat: [f64; 4],
    pub acc_quat: [f64; 4],
    pub delta: f64,
    pub rest_detected: bool,
    pub mag_dist_detected: bool,
    pub last_acc_lp: [f64; 3],
    pub acc_lp_state: [[f64; 3]; 2],
    pub k_mag_init: f64,
    pub last_mag_dis_angle: f64,
    pub last_mag_corr_angular_rate: f64,
    pub bias: [f64; 3],
    pub bias_p: Matrix3<f64>,
    pub motion_bias_est_r_lp_state: [[f64; 9]; 2],
    pub motion_bias_est_bias_lp_state: [[f64; 2]; 2],
    pub rest_last_squared_deviations: [f64; 2],
    pub rest_t: f64,
    pub rest_last_gyr_lp: [f64; 3],
    pub rest_gyr_lp_state: [[f64; 3]; 2],
    pub rest_last_acc_lp: [f64; 3],
    pub rest_acc_lp_state: [[f64; 3]; 2],
    pub mag_ref_norm: f64,
    pub mag_ref_dip: f64,
    pub mag_undisturbed_t: f64,
    pub mag_reject_t: f64,
    pub mag_candidate_norm: f64,
    pub mag_candidate_dip: f64,
    pub mag_candidate_t: f64,
    pub mag_norm_dip: [f64; 2],
    pub mag_norm_dip_lp_state: [[f64; 2]; 2],
    pub mag_seen: bool,
}

impl Default for VqfState {
    fn default() -> Self {
        Self {
            gyr_quat: [1.0, 0.0, 0.0, 0.0],
            acc_quat: [1.0, 0.0, 0.0, 0.0],
            delta: 0.0,
            rest_detected: false,
            mag_dist_detected: true,
            last_acc_lp: [0.0; 3],
            acc_lp_state: [[f64::NAN; 3]; 2],
            k_mag_init: 1.0,
            last_mag_dis_angle: 0.0,
            last_mag_corr_angular_rate: 0.0,
            bias: [0.0; 3],
            bias_p: Matrix3::repeat(f64::NAN),
            motion_bias_est_r_lp_state: [[f64::NAN; 9]; 2],
            motion_bias_est_bias_lp_state: [[f64::NAN; 2]; 2],
            rest_last_squared_deviations: [0.0; 2],
            rest_t: 0.0,
            rest_last_gyr_lp: [0.0; 3],
            rest_gyr_lp_state: [[f64::NAN; 3]; 2],
            rest_last_acc_lp: [0.0; 3],
            rest_acc_lp_state: [[f64::NAN; 3]; 2],
            mag_ref_norm: 0.0,
            mag_ref_dip: 0.0,
            mag_undisturbed_t: 0.0,
            mag_reject_t: -1.0,
            mag_candidate_norm: -1.0,
            mag_candidate_dip: 0.0,
            mag_candidate_t: 0.0,
            mag_norm_dip: [0.0; 2],
            mag_norm_dip_lp_state: [[f64::NAN; 2]; 2],
            mag_seen: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VqfCoeffs {
    pub gyr_ts: f64,
    pub acc_ts: f64,
    pub mag_ts: f64,
    pub acc_lp_b: [f64; 3],
    pub acc_lp_a: [f64; 2],
    pub k_mag: f64,
    pub bias_p0: f64,
    pub bias_v: f64,
    pub bias_motion_w: f64,
    pub bias_vertical_w: f64,
    pub bias_rest_w: f64,
    pub rest_gyr_lp_b: [f64; 3],
    pub rest_gyr_lp_a: [f64; 2],
    pub rest_acc_lp_b: [f64; 3],
    pub rest_acc_lp_a: [f64; 2],
    pub k_mag_ref: f64,
    pub mag_norm_dip_lp_b: [f64; 3],
    pub mag_norm_dip_lp_a: [f64; 2],
}

pub struct Vqf {
    pub(crate) params: VqfParams,
    pub(crate) state: VqfState,
    pub(crate) coeffs: VqfCoeffs,
}

// ── Filter helpers (biquad LPF) ──────────────────────────────────────────────

/// Compute biquad LPF coefficients (b[3], a[2]) for time constant `tau` and sample period `Ts`.
/// 2nd-order Butterworth.
pub(crate) fn filter_coeffs(tau: f64, ts: f64) -> ([f64; 3], [f64; 2]) {
    if tau <= 0.0 {
        return ([1.0, 0.0, 0.0], [0.0, 0.0]);
    }
    let fc = (2.0_f64.sqrt()) / (2.0 * PI) / tau;
    let c = (PI * fc * ts).tan();
    let d = c * c + (2.0_f64).sqrt() * c + 1.0;
    let b0 = c * c / d;
    let b1 = 2.0 * b0;
    let b2 = b0;
    let a1 = 2.0 * (c * c - 1.0) / d;
    let a2 = (c * c - (2.0_f64).sqrt() * c + 1.0) / d;
    ([b0, b1, b2], [a1, a2])
}

/// Initial filter state for a constant input `x0`.
pub(crate) fn filter_initial_state(x0: f64, b: [f64; 3], a: [f64; 2]) -> [f64; 2] {
    [x0 * (1.0 - b[0]), x0 * (b[2] - a[1])]
}

/// Single-sample biquad step (direct-form-II transposed).
pub(crate) fn filter_step(x: f64, b: [f64; 3], a: [f64; 2], state: &mut [f64; 2]) -> f64 {
    let y = b[0] * x + state[0];
    state[0] = b[1] * x - a[0] * y + state[1];
    state[1] = b[2] * x - a[1] * y;
    y
}

/// Adapt biquad state when coefficients change mid-run, preserving filter output continuity.
pub(crate) fn filter_adapt_state_for_coeff_change(
    last_y: f64,
    _b_old: [f64; 3],
    _a_old: [f64; 2],
    b_new: [f64; 3],
    a_new: [f64; 2],
    state: &mut [f64; 2],
) {
    if state[0].is_nan() {
        return;
    }
    state[0] = last_y * (1.0 - b_new[0]);
    state[1] = last_y * (b_new[2] - a_new[1]);
}

/// Filter a vector componentwise, lazy-initializing per-component state on first call.
fn filter_vec_n<const N: usize>(
    x: &[f64; N],
    b: [f64; 3],
    a: [f64; 2],
    state: &mut [[f64; N]; 2],
) -> [f64; N] {
    let mut out = [0.0; N];
    for i in 0..N {
        if state[0][i].is_nan() {
            let init = filter_initial_state(x[i], b, a);
            state[0][i] = init[0];
            state[1][i] = init[1];
            out[i] = x[i];
        } else {
            let mut s = [state[0][i], state[1][i]];
            out[i] = filter_step(x[i], b, a, &mut s);
            state[0][i] = s[0];
            state[1][i] = s[1];
        }
    }
    out
}

// ── Quaternion helpers ───────────────────────────────────────────────────────

#[inline]
pub(crate) fn quat_multiply(q1: [f64; 4], q2: [f64; 4]) -> [f64; 4] {
    [
        q1[0] * q2[0] - q1[1] * q2[1] - q1[2] * q2[2] - q1[3] * q2[3],
        q1[0] * q2[1] + q1[1] * q2[0] + q1[2] * q2[3] - q1[3] * q2[2],
        q1[0] * q2[2] - q1[1] * q2[3] + q1[2] * q2[0] + q1[3] * q2[1],
        q1[0] * q2[3] + q1[1] * q2[2] - q1[2] * q2[1] + q1[3] * q2[0],
    ]
}

#[inline]
pub(crate) fn quat_conj(q: [f64; 4]) -> [f64; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}

#[inline]
pub(crate) fn quat_rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let v_q = [0.0, v[0], v[1], v[2]];
    let r = quat_multiply(quat_multiply(q, v_q), quat_conj(q));
    [r[1], r[2], r[3]]
}

#[inline]
pub(crate) fn quat_normalize(q: [f64; 4]) -> [f64; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < EPS {
        return [1.0, 0.0, 0.0, 0.0];
    }
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

#[inline]
pub(crate) fn quat_apply_delta(q: [f64; 4], delta: f64) -> [f64; 4] {
    let c = (delta * 0.5).cos();
    let s = (delta * 0.5).sin();
    quat_multiply([c, 0.0, 0.0, s], q)
}

#[inline]
fn quat_to_rotation_matrix(q: [f64; 4]) -> [[f64; 3]; 3] {
    let w = q[0];
    let x = q[1];
    let y = q[2];
    let z = q[3];
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

// ── Vqf core ─────────────────────────────────────────────────────────────────

impl Vqf {
    pub fn new(gyr_ts: f64) -> Self {
        Self::with_timesteps(gyr_ts, gyr_ts, gyr_ts)
    }

    pub fn with_timesteps(gyr_ts: f64, acc_ts: f64, mag_ts: f64) -> Self {
        Self::with_params_and_timesteps(VqfParams::default(), gyr_ts, acc_ts, mag_ts)
    }

    pub fn with_params(gyr_ts: f64, params: VqfParams) -> Self {
        Self::with_params_and_timesteps(params, gyr_ts, gyr_ts, gyr_ts)
    }

    fn with_params_and_timesteps(params: VqfParams, gyr_ts: f64, acc_ts: f64, mag_ts: f64) -> Self {
        let mut v = Self {
            params,
            state: VqfState::default(),
            coeffs: VqfCoeffs {
                gyr_ts,
                acc_ts,
                mag_ts,
                ..Default::default()
            },
        };
        v.setup();
        v
    }

    pub fn reset_state(&mut self) {
        self.state = VqfState::default();
        self.setup();
    }

    pub(crate) fn setup(&mut self) {
        let p = &self.params;

        let (acc_b, acc_a) = filter_coeffs(p.tau_acc, self.coeffs.acc_ts);
        self.coeffs.acc_lp_b = acc_b;
        self.coeffs.acc_lp_a = acc_a;

        self.coeffs.k_mag = if p.tau_mag <= 0.0 {
            0.0
        } else {
            1.0 - (-self.coeffs.mag_ts / p.tau_mag).exp()
        };

        let bias_sigma_init_rad = p.bias_sigma_init * PI / 180.0;
        self.coeffs.bias_p0 = bias_sigma_init_rad * bias_sigma_init_rad;
        self.coeffs.bias_v = self.coeffs.bias_p0 * self.coeffs.gyr_ts / p.bias_forgetting_time;

        let bias_sigma_motion_rad = p.bias_sigma_motion * PI / 180.0;
        self.coeffs.bias_motion_w =
            bias_sigma_motion_rad * bias_sigma_motion_rad / self.coeffs.gyr_ts;
        self.coeffs.bias_vertical_w =
            self.coeffs.bias_motion_w / p.bias_vertical_forgetting_factor.max(1e-12);

        let bias_sigma_rest_rad = p.bias_sigma_rest * PI / 180.0;
        self.coeffs.bias_rest_w = bias_sigma_rest_rad * bias_sigma_rest_rad / self.coeffs.gyr_ts;

        let (rest_gyr_b, rest_gyr_a) = filter_coeffs(p.rest_filter_tau, self.coeffs.gyr_ts);
        self.coeffs.rest_gyr_lp_b = rest_gyr_b;
        self.coeffs.rest_gyr_lp_a = rest_gyr_a;

        let (rest_acc_b, rest_acc_a) = filter_coeffs(p.rest_filter_tau, self.coeffs.acc_ts);
        self.coeffs.rest_acc_lp_b = rest_acc_b;
        self.coeffs.rest_acc_lp_a = rest_acc_a;

        self.coeffs.k_mag_ref = if p.mag_ref_tau <= 0.0 {
            0.0
        } else {
            1.0 - (-self.coeffs.mag_ts / p.mag_ref_tau).exp()
        };

        let (mag_b, mag_a) = filter_coeffs(p.mag_current_tau, self.coeffs.mag_ts);
        self.coeffs.mag_norm_dip_lp_b = mag_b;
        self.coeffs.mag_norm_dip_lp_a = mag_a;

        self.state.bias_p = Matrix3::from_diagonal_element(self.coeffs.bias_p0);
    }

    fn update_gyr(&mut self, gyr: [f64; 3]) {
        if self.params.rest_bias_est_enabled || self.params.mag_dist_rejection_enabled {
            let gyr_lp = filter_vec_n(
                &gyr,
                self.coeffs.rest_gyr_lp_b,
                self.coeffs.rest_gyr_lp_a,
                &mut self.state.rest_gyr_lp_state,
            );
            let dev = [gyr[0] - gyr_lp[0], gyr[1] - gyr_lp[1], gyr[2] - gyr_lp[2]];
            let sq_dev = dev[0] * dev[0] + dev[1] * dev[1] + dev[2] * dev[2];
            let bias_clip = self.params.bias_clip * PI / 180.0;
            let max_abs = gyr_lp.iter().fold(0.0_f64, |m, &v| m.max(v.abs()));
            let rest_th_gyr_rad = self.params.rest_th_gyr * PI / 180.0;
            if sq_dev >= rest_th_gyr_rad * rest_th_gyr_rad || max_abs > bias_clip {
                self.state.rest_t = 0.0;
                self.state.rest_detected = false;
            }
            self.state.rest_last_gyr_lp = gyr_lp;
            self.state.rest_last_squared_deviations[0] = sq_dev;
        }

        let g = [
            gyr[0] - self.state.bias[0],
            gyr[1] - self.state.bias[1],
            gyr[2] - self.state.bias[2],
        ];
        let g_norm = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
        let angle = g_norm * self.coeffs.gyr_ts;
        if g_norm > EPS {
            let c = (angle * 0.5).cos();
            let s = (angle * 0.5).sin() / g_norm;
            let step_q = [c, s * g[0], s * g[1], s * g[2]];
            self.state.gyr_quat = quat_normalize(quat_multiply(self.state.gyr_quat, step_q));
        }
    }

    fn update_acc(&mut self, acc: [f64; 3]) {
        if acc == [0.0, 0.0, 0.0] {
            return;
        }
        let acc_ts = self.coeffs.acc_ts;

        if self.params.rest_bias_est_enabled {
            let acc_lp = filter_vec_n(
                &acc,
                self.coeffs.rest_acc_lp_b,
                self.coeffs.rest_acc_lp_a,
                &mut self.state.rest_acc_lp_state,
            );
            let dev = [acc[0] - acc_lp[0], acc[1] - acc_lp[1], acc[2] - acc_lp[2]];
            let sq_dev = dev[0] * dev[0] + dev[1] * dev[1] + dev[2] * dev[2];
            if sq_dev >= self.params.rest_th_acc * self.params.rest_th_acc {
                self.state.rest_t = 0.0;
                self.state.rest_detected = false;
            } else {
                self.state.rest_t += acc_ts;
                if self.state.rest_t >= self.params.rest_min_t {
                    self.state.rest_detected = true;
                }
            }
            self.state.rest_last_acc_lp = acc_lp;
            self.state.rest_last_squared_deviations[1] = sq_dev;
        }

        let acc_earth = quat_rotate(self.state.gyr_quat, acc);
        self.state.last_acc_lp = filter_vec_n(
            &acc_earth,
            self.coeffs.acc_lp_b,
            self.coeffs.acc_lp_a,
            &mut self.state.acc_lp_state,
        );

        let acc_e6 = quat_rotate(self.state.acc_quat, self.state.last_acc_lp);
        let q_w_inner = ((acc_e6[2] + 1.0) / 2.0).max(0.0);
        let q_w = q_w_inner.sqrt();
        let acc_corr = if q_w > EPS {
            [q_w, 0.5 * acc_e6[1] / q_w, -0.5 * acc_e6[0] / q_w, 0.0]
        } else {
            [0.0, 1.0, 0.0, 0.0]
        };
        self.state.acc_quat = quat_normalize(quat_multiply(acc_corr, self.state.acc_quat));

        self.update_bias_kalman(acc_earth);
    }

    fn update_bias_kalman(&mut self, acc_earth: [f64; 3]) {
        if !self.params.motion_bias_est_enabled && !self.params.rest_bias_est_enabled {
            return;
        }
        let bias_clip = self.params.bias_clip * PI / 180.0;
        let mut bias = self.state.bias;
        let q6 = self.quat_6d_internal();
        let r = quat_to_rotation_matrix(q6);

        let r_bias = [
            r[0][0] * bias[0] + r[0][1] * bias[1] + r[0][2] * bias[2],
            r[1][0] * bias[0] + r[1][1] * bias[1] + r[1][2] * bias[2],
            r[2][0] * bias[0] + r[2][1] * bias[1] + r[2][2] * bias[2],
        ];
        let bias_lp_in = [r_bias[0], r_bias[1]];

        let bias_lp = filter_vec_n(
            &bias_lp_in,
            self.coeffs.acc_lp_b,
            self.coeffs.acc_lp_a,
            &mut self.state.motion_bias_est_bias_lp_state,
        );

        let r_flat = [
            r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
        ];
        let r_filt_arr = filter_vec_n(
            &r_flat,
            self.coeffs.acc_lp_b,
            self.coeffs.acc_lp_a,
            &mut self.state.motion_bias_est_r_lp_state,
        );

        let acc_ts = self.coeffs.acc_ts;
        let (e_opt, w_opt, r_meas): (Option<[f64; 3]>, Option<[f64; 3]>, Matrix3<f64>) =
            if self.state.rest_detected && self.params.rest_bias_est_enabled {
                let e = [
                    self.state.rest_last_gyr_lp[0] - bias[0],
                    self.state.rest_last_gyr_lp[1] - bias[1],
                    self.state.rest_last_gyr_lp[2] - bias[2],
                ];
                (
                    Some(e),
                    Some([self.coeffs.bias_rest_w; 3]),
                    Matrix3::identity(),
                )
            } else if self.params.motion_bias_est_enabled {
                let e = [
                    -acc_earth[1] / acc_ts + bias_lp[0]
                        - r_filt_arr[0] * bias[0]
                        - r_filt_arr[1] * bias[1]
                        - r_filt_arr[2] * bias[2],
                    acc_earth[0] / acc_ts + bias_lp[1]
                        - r_filt_arr[3] * bias[0]
                        - r_filt_arr[4] * bias[1]
                        - r_filt_arr[5] * bias[2],
                    -r_filt_arr[6] * bias[0] - r_filt_arr[7] * bias[1] - r_filt_arr[8] * bias[2],
                ];
                let r_mat = Matrix3::new(
                    r_filt_arr[0],
                    r_filt_arr[1],
                    r_filt_arr[2],
                    r_filt_arr[3],
                    r_filt_arr[4],
                    r_filt_arr[5],
                    r_filt_arr[6],
                    r_filt_arr[7],
                    r_filt_arr[8],
                );
                (
                    Some(e),
                    Some([
                        self.coeffs.bias_motion_w,
                        self.coeffs.bias_motion_w,
                        self.coeffs.bias_vertical_w,
                    ]),
                    r_mat,
                )
            } else {
                (None, None, Matrix3::identity())
            };

        for i in 0..3 {
            if self.state.bias_p[(i, i)] < self.coeffs.bias_p0 {
                let v = self.state.bias_p[(i, i)] + self.coeffs.bias_v;
                self.state.bias_p[(i, i)] = v.min(self.coeffs.bias_p0);
            }
        }

        if let (Some(e), Some(w)) = (e_opt, w_opt) {
            let e_clip = NaVec3::new(
                e[0].clamp(-bias_clip, bias_clip),
                e[1].clamp(-bias_clip, bias_clip),
                e[2].clamp(-bias_clip, bias_clip),
            );
            let w_diag = Matrix3::from_diagonal(&NaVec3::new(w[0], w[1], w[2]));
            let r_t = r_meas.transpose();
            let s_mat = w_diag + r_meas * self.state.bias_p * r_t;
            let s_inv = match s_mat.pseudo_inverse(EPS) {
                Ok(m) => m,
                Err(_) => return,
            };
            let k_gain = self.state.bias_p * r_t * s_inv;
            let bias_v = NaVec3::new(bias[0], bias[1], bias[2]) + k_gain * e_clip;
            bias = [
                bias_v[0].clamp(-bias_clip, bias_clip),
                bias_v[1].clamp(-bias_clip, bias_clip),
                bias_v[2].clamp(-bias_clip, bias_clip),
            ];
            self.state.bias_p -= k_gain * r_meas * self.state.bias_p;
        }
        self.state.bias = bias;
    }

    fn update_mag(&mut self, mag: [f64; 3]) {
        if mag == [0.0, 0.0, 0.0] {
            return;
        }
        self.state.mag_seen = true;
        let mag_ts = self.coeffs.mag_ts;
        let q6 = self.quat_6d_internal();
        let mag_earth = quat_rotate(q6, mag);

        if self.params.mag_dist_rejection_enabled {
            let mag_norm = (mag_earth[0] * mag_earth[0]
                + mag_earth[1] * mag_earth[1]
                + mag_earth[2] * mag_earth[2])
                .sqrt();
            let mag_dip = -(mag_earth[2] / mag_norm.max(EPS)).asin();
            let nd_in = [mag_norm, mag_dip];
            let mag_norm_dip = if self.params.mag_current_tau > 0.0 {
                filter_vec_n(
                    &nd_in,
                    self.coeffs.mag_norm_dip_lp_b,
                    self.coeffs.mag_norm_dip_lp_a,
                    &mut self.state.mag_norm_dip_lp_state,
                )
            } else {
                nd_in
            };
            self.state.mag_norm_dip = mag_norm_dip;

            let dip_th = self.params.mag_dip_th * PI / 180.0;
            if (mag_norm_dip[0] - self.state.mag_ref_norm).abs()
                < self.params.mag_norm_th * self.state.mag_ref_norm
                && (mag_norm_dip[1] - self.state.mag_ref_dip).abs() < dip_th
            {
                self.state.mag_undisturbed_t += mag_ts;
                if self.state.mag_undisturbed_t >= self.params.mag_min_undisturbed_time {
                    self.state.mag_dist_detected = false;
                    self.state.mag_ref_norm +=
                        self.coeffs.k_mag_ref * (mag_norm_dip[0] - self.state.mag_ref_norm);
                    self.state.mag_ref_dip +=
                        self.coeffs.k_mag_ref * (mag_norm_dip[1] - self.state.mag_ref_dip);
                }
            } else {
                self.state.mag_undisturbed_t = 0.0;
                self.state.mag_dist_detected = true;
            }

            if (mag_norm_dip[0] - self.state.mag_candidate_norm).abs()
                < self.params.mag_norm_th * self.state.mag_candidate_norm
                && (mag_norm_dip[1] - self.state.mag_candidate_dip).abs() < dip_th
            {
                let gyr_norm = (self.state.rest_last_gyr_lp[0].powi(2)
                    + self.state.rest_last_gyr_lp[1].powi(2)
                    + self.state.rest_last_gyr_lp[2].powi(2))
                .sqrt();
                if gyr_norm >= self.params.mag_new_min_gyr * PI / 180.0 {
                    self.state.mag_candidate_t += mag_ts;
                }
                self.state.mag_candidate_norm +=
                    self.coeffs.k_mag_ref * (mag_norm_dip[0] - self.state.mag_candidate_norm);
                self.state.mag_candidate_dip +=
                    self.coeffs.k_mag_ref * (mag_norm_dip[1] - self.state.mag_candidate_dip);
                if self.state.mag_dist_detected
                    && (self.state.mag_candidate_t >= self.params.mag_new_time
                        || (self.state.mag_ref_norm == 0.0
                            && self.state.mag_candidate_t >= self.params.mag_new_first_time))
                {
                    self.state.mag_ref_norm = self.state.mag_candidate_norm;
                    self.state.mag_ref_dip = self.state.mag_candidate_dip;
                    self.state.mag_dist_detected = false;
                    self.state.mag_undisturbed_t = self.params.mag_min_undisturbed_time;
                }
            } else {
                self.state.mag_candidate_t = 0.0;
                self.state.mag_candidate_norm = mag_norm_dip[0];
                self.state.mag_candidate_dip = mag_norm_dip[1];
            }
        }

        let mut dis = mag_earth[0].atan2(mag_earth[1]) - self.state.delta;
        if dis > PI {
            dis -= 2.0 * PI;
        } else if dis < -PI {
            dis += 2.0 * PI;
        }
        self.state.last_mag_dis_angle = dis;

        let mut k = self.coeffs.k_mag;
        if self.params.mag_dist_rejection_enabled {
            if self.state.mag_dist_detected {
                if self.state.mag_reject_t <= self.params.mag_max_rejection_time {
                    self.state.mag_reject_t += mag_ts;
                    k = 0.0;
                } else {
                    k /= self.params.mag_rejection_factor;
                }
            } else {
                self.state.mag_reject_t =
                    (self.state.mag_reject_t - self.params.mag_rejection_factor * mag_ts).max(0.0);
            }
        }

        if self.state.k_mag_init != 0.0 {
            if k < self.state.k_mag_init {
                k = self.state.k_mag_init;
            }
            self.state.k_mag_init = self.state.k_mag_init / (self.state.k_mag_init + 1.0);
            if self.state.k_mag_init * self.params.tau_mag < self.coeffs.mag_ts {
                self.state.k_mag_init = 0.0;
            }
        }

        self.state.delta += k * dis;
        self.state.last_mag_corr_angular_rate = k * dis / self.coeffs.mag_ts;

        if self.state.delta > PI {
            self.state.delta -= 2.0 * PI;
        } else if self.state.delta < -PI {
            self.state.delta += 2.0 * PI;
        }
    }

    pub fn update(&mut self, gyro: [f64; 3], accel: [f64; 3], mag: Option<[f64; 3]>) {
        self.update_gyr(gyro);
        self.update_acc(accel);
        if let Some(m) = mag {
            self.update_mag(m);
        }
    }

    pub fn quat_3d(&self) -> [f64; 4] {
        self.state.gyr_quat
    }

    pub fn quat_6d(&self) -> [f64; 4] {
        self.quat_6d_internal()
    }

    pub fn quat_9d(&self) -> [f64; 4] {
        assert!(
            self.state.mag_seen,
            "quat_9d called before any magnetometer update — feed mag first"
        );
        let q6 = self.quat_6d_internal();
        quat_apply_delta(q6, self.state.delta)
    }

    fn quat_6d_internal(&self) -> [f64; 4] {
        quat_normalize(quat_multiply(self.state.acc_quat, self.state.gyr_quat))
    }

    pub fn rest_detected(&self) -> bool {
        self.state.rest_detected
    }

    pub fn mag_dist_detected(&self) -> bool {
        self.state.mag_dist_detected
    }

    pub fn set_tau_acc(&mut self, tau: f64) {
        let (b_old, a_old) = (self.coeffs.acc_lp_b, self.coeffs.acc_lp_a);
        self.params.tau_acc = tau;
        let (b_new, a_new) = filter_coeffs(tau, self.coeffs.acc_ts);
        for i in 0..3 {
            let mut s = [self.state.acc_lp_state[0][i], self.state.acc_lp_state[1][i]];
            filter_adapt_state_for_coeff_change(
                self.state.last_acc_lp[i],
                b_old,
                a_old,
                b_new,
                a_new,
                &mut s,
            );
            self.state.acc_lp_state[0][i] = s[0];
            self.state.acc_lp_state[1][i] = s[1];
        }
        self.coeffs.acc_lp_b = b_new;
        self.coeffs.acc_lp_a = a_new;
    }

    pub fn set_tau_mag(&mut self, tau: f64) {
        self.params.tau_mag = tau;
        self.coeffs.k_mag = if tau <= 0.0 {
            0.0
        } else {
            1.0 - (-self.coeffs.mag_ts / tau).exp()
        };
    }

    /// Returns (bias_estimate_rad_per_sec, sigma).
    pub fn bias_estimate(&self) -> ([f64; 3], f64) {
        let trace =
            self.state.bias_p[(0, 0)] + self.state.bias_p[(1, 1)] + self.state.bias_p[(2, 2)];
        let sigma = (trace / 3.0).sqrt();
        (self.state.bias, sigma)
    }

    /// Sprint 5 hookpoint for per-MAC bias persistence.
    pub fn set_bias_estimate(&mut self, bias: [f64; 3], sigma: Option<f64>) {
        let bias_clip = self.params.bias_clip * PI / 180.0;
        self.state.bias = [
            bias[0].clamp(-bias_clip, bias_clip),
            bias[1].clamp(-bias_clip, bias_clip),
            bias[2].clamp(-bias_clip, bias_clip),
        ];
        if let Some(sigma) = sigma {
            let var = sigma * sigma;
            self.state.bias_p = Matrix3::from_diagonal_element(var);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_match() {
        let p = VqfParams::default();
        assert_eq!(p.tau_acc, 3.0);
        assert_eq!(p.tau_mag, 9.0);
        assert_eq!(p.bias_clip, 2.0);
        assert!(p.motion_bias_est_enabled);
    }

    #[test]
    fn setup_initializes_bias_p_diagonal() {
        let v = Vqf::new(1.0 / 200.0);
        let p0 = v.coeffs.bias_p0;
        assert!(p0 > 0.0);
        assert_eq!(v.state.bias_p[(0, 0)], p0);
        assert_eq!(v.state.bias_p[(1, 1)], p0);
        assert_eq!(v.state.bias_p[(2, 2)], p0);
    }

    #[test]
    fn quat_multiply_identity() {
        let id = [1.0, 0.0, 0.0, 0.0];
        let q = [0.5, 0.5, 0.5, 0.5];
        assert_eq!(quat_multiply(id, q), q);
    }

    #[test]
    fn quat_normalize_zero_yields_identity() {
        let n = quat_normalize([0.0, 0.0, 0.0, 0.0]);
        assert_eq!(n, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn filter_step_unit_input_steady_state() {
        let (b, a) = filter_coeffs(0.5, 0.01);
        let mut state = filter_initial_state(1.0, b, a);
        for _ in 0..1000 {
            let y = filter_step(1.0, b, a, &mut state);
            assert!((y - 1.0).abs() < 1e-6, "y = {y}");
        }
    }
}
