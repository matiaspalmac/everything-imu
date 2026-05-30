//! Madgwick AHRS implementation. f32 throughout.

pub struct Madgwick {
    pub sample_period: f32,
    pub beta: f32,
    pub quat: [f32; 4], // [w, x, y, z]
}

impl Madgwick {
    pub fn new(sample_period: f32) -> Self {
        Self::with_beta(sample_period, 1.0)
    }

    pub fn with_beta(sample_period: f32, beta: f32) -> Self {
        Self {
            sample_period,
            beta,
            quat: [1.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn quaternion(&self) -> [f32; 4] {
        self.quat
    }

    pub fn reset(&mut self) {
        self.quat = [1.0, 0.0, 0.0, 0.0];
    }

    /// Update the integration timestep live, preserving the orientation
    /// estimate. Madgwick multiplies the quaternion derivative by
    /// `sample_period` each step, so this must match the real delivered sample
    /// cadence or the integrated rotation skews.
    pub fn set_sample_period(&mut self, sample_period: f32) {
        self.sample_period = sample_period;
    }

    /// 6D update — gyroscope + accelerometer only.
    /// `MadgwickAHRSupdateIMU(g_x, g_y, g_z, a_x, a_y, a_z)`.
    /// Gyro in rad/s, accel in any consistent units (normalized internally).
    pub fn update_imu(&mut self, gx: f32, gy: f32, gz: f32, ax: f32, ay: f32, az: f32) {
        let mut q1 = self.quat[0];
        let mut q2 = self.quat[1];
        let mut q3 = self.quat[2];
        let mut q4 = self.quat[3];

        let mut norm = (ax * ax + ay * ay + az * az).sqrt();
        if norm == 0.0 {
            return;
        }
        norm = 1.0 / norm;
        let ax = ax * norm;
        let ay = ay * norm;
        let az = az * norm;

        let _2q1 = 2.0 * q1;
        let _2q2 = 2.0 * q2;
        let _2q3 = 2.0 * q3;
        let _2q4 = 2.0 * q4;
        let _4q1 = 4.0 * q1;
        let _4q2 = 4.0 * q2;
        let _4q3 = 4.0 * q3;
        let _8q2 = 8.0 * q2;
        let _8q3 = 8.0 * q3;
        let q1q1 = q1 * q1;
        let q2q2 = q2 * q2;
        let q3q3 = q3 * q3;
        let q4q4 = q4 * q4;

        let s1 = _4q1 * q3q3 + _2q3 * ax + _4q1 * q2q2 - _2q2 * ay;
        let s2 = _4q2 * q4q4 - _2q4 * ax + 4.0 * q1q1 * q2 - _2q1 * ay - _4q2
            + _8q2 * q2q2
            + _8q2 * q3q3
            + _4q2 * az;
        let s3 = 4.0 * q1q1 * q3 + _2q1 * ax + _4q3 * q4q4 - _2q4 * ay - _4q3
            + _8q3 * q2q2
            + _8q3 * q3q3
            + _4q3 * az;
        let s4 = 4.0 * q2q2 * q4 - _2q2 * ax + 4.0 * q3q3 * q4 - _2q3 * ay;

        let s_norm = (s1 * s1 + s2 * s2 + s3 * s3 + s4 * s4).sqrt();
        let inv = if s_norm == 0.0 { 0.0 } else { 1.0 / s_norm };
        let s1 = s1 * inv;
        let s2 = s2 * inv;
        let s3 = s3 * inv;
        let s4 = s4 * inv;

        let q_dot1 = 0.5 * (-q2 * gx - q3 * gy - q4 * gz) - self.beta * s1;
        let q_dot2 = 0.5 * (q1 * gx + q3 * gz - q4 * gy) - self.beta * s2;
        let q_dot3 = 0.5 * (q1 * gy - q2 * gz + q4 * gx) - self.beta * s3;
        let q_dot4 = 0.5 * (q1 * gz + q2 * gy - q3 * gx) - self.beta * s4;

        q1 += q_dot1 * self.sample_period;
        q2 += q_dot2 * self.sample_period;
        q3 += q_dot3 * self.sample_period;
        q4 += q_dot4 * self.sample_period;

        let qn = (q1 * q1 + q2 * q2 + q3 * q3 + q4 * q4).sqrt();
        if qn == 0.0 {
            return;
        }
        let inv = 1.0 / qn;
        self.quat = [q1 * inv, q2 * inv, q3 * inv, q4 * inv];
    }

    /// 9D update — gyro + accel + mag.
    /// `MadgwickAHRSupdate(g_x, g_y, g_z, a_x, a_y, a_z, m_x, m_y, m_z)`.
    /// Kept as 9 individual args.
    #[allow(clippy::too_many_arguments)]
    pub fn update_marg(
        &mut self,
        gx: f32,
        gy: f32,
        gz: f32,
        ax: f32,
        ay: f32,
        az: f32,
        mx: f32,
        my: f32,
        mz: f32,
    ) {
        let mut q1 = self.quat[0];
        let mut q2 = self.quat[1];
        let mut q3 = self.quat[2];
        let mut q4 = self.quat[3];

        let mut norm = (ax * ax + ay * ay + az * az).sqrt();
        if norm == 0.0 {
            return;
        }
        norm = 1.0 / norm;
        let ax = ax * norm;
        let ay = ay * norm;
        let az = az * norm;

        let mut norm = (mx * mx + my * my + mz * mz).sqrt();
        if norm == 0.0 {
            return;
        }
        norm = 1.0 / norm;
        let mx = mx * norm;
        let my = my * norm;
        let mz = mz * norm;

        let _2q1 = 2.0 * q1;
        let _2q2 = 2.0 * q2;
        let _2q3 = 2.0 * q3;
        let _2q4 = 2.0 * q4;
        let _2q1q3 = 2.0 * q1 * q3;
        let _2q3q4 = 2.0 * q3 * q4;
        let q1q1 = q1 * q1;
        let q1q2 = q1 * q2;
        let q1q3 = q1 * q3;
        let q1q4 = q1 * q4;
        let q2q2 = q2 * q2;
        let q2q3 = q2 * q3;
        let q2q4 = q2 * q4;
        let q3q3 = q3 * q3;
        let q3q4 = q3 * q4;
        let q4q4 = q4 * q4;

        let _2q1mx = 2.0 * q1 * mx;
        let _2q1my = 2.0 * q1 * my;
        let _2q1mz = 2.0 * q1 * mz;
        let _2q2mx = 2.0 * q2 * mx;
        let hx =
            mx * q1q1 - _2q1my * q4 + _2q1mz * q3 + mx * q2q2 + _2q2 * my * q3 + _2q2 * mz * q4
                - mx * q3q3
                - mx * q4q4;
        let hy = _2q1mx * q4 + my * q1q1 - _2q1mz * q2 + _2q2mx * q3 - my * q2q2
            + my * q3q3
            + _2q3 * mz * q4
            - my * q4q4;
        let _2bx = (hx * hx + hy * hy).sqrt();
        let _2bz = -_2q1mx * q3 + _2q1my * q2 + mz * q1q1 + _2q2mx * q4 - mz * q2q2
            + _2q3 * my * q4
            - mz * q3q3
            + mz * q4q4;
        let _4bx = 2.0 * _2bx;
        let _4bz = 2.0 * _2bz;

        let s1 = -_2q3 * (2.0 * q2q4 - _2q1q3 - ax) + _2q2 * (2.0 * q1q2 + _2q3q4 - ay)
            - _2bz * q3 * (_2bx * (0.5 - q3q3 - q4q4) + _2bz * (q2q4 - q1q3) - mx)
            + (-_2bx * q4 + _2bz * q2) * (_2bx * (q2q3 - q1q4) + _2bz * (q1q2 + q3q4) - my)
            + _2bx * q3 * (_2bx * (q1q3 + q2q4) + _2bz * (0.5 - q2q2 - q3q3) - mz);
        let s2 = _2q4 * (2.0 * q2q4 - _2q1q3 - ax) + _2q1 * (2.0 * q1q2 + _2q3q4 - ay)
            - 4.0 * q2 * (1.0 - 2.0 * q2q2 - 2.0 * q3q3 - az)
            + _2bz * q4 * (_2bx * (0.5 - q3q3 - q4q4) + _2bz * (q2q4 - q1q3) - mx)
            + (_2bx * q3 + _2bz * q1) * (_2bx * (q2q3 - q1q4) + _2bz * (q1q2 + q3q4) - my)
            + (_2bx * q4 - _4bz * q2) * (_2bx * (q1q3 + q2q4) + _2bz * (0.5 - q2q2 - q3q3) - mz);
        let s3 = -_2q1 * (2.0 * q2q4 - _2q1q3 - ax) + _2q4 * (2.0 * q1q2 + _2q3q4 - ay)
            - 4.0 * q3 * (1.0 - 2.0 * q2q2 - 2.0 * q3q3 - az)
            + (-_4bx * q3 - _2bz * q1) * (_2bx * (0.5 - q3q3 - q4q4) + _2bz * (q2q4 - q1q3) - mx)
            + (_2bx * q2 + _2bz * q4) * (_2bx * (q2q3 - q1q4) + _2bz * (q1q2 + q3q4) - my)
            + (_2bx * q1 - _4bz * q3) * (_2bx * (q1q3 + q2q4) + _2bz * (0.5 - q2q2 - q3q3) - mz);
        let s4 = _2q2 * (2.0 * q2q4 - _2q1q3 - ax)
            + _2q3 * (2.0 * q1q2 + _2q3q4 - ay)
            + (-_4bx * q4 + _2bz * q2) * (_2bx * (0.5 - q3q3 - q4q4) + _2bz * (q2q4 - q1q3) - mx)
            + (-_2bx * q1 + _2bz * q3) * (_2bx * (q2q3 - q1q4) + _2bz * (q1q2 + q3q4) - my)
            + _2bx * q2 * (_2bx * (q1q3 + q2q4) + _2bz * (0.5 - q2q2 - q3q3) - mz);

        let s_norm = (s1 * s1 + s2 * s2 + s3 * s3 + s4 * s4).sqrt();
        let inv = if s_norm == 0.0 { 0.0 } else { 1.0 / s_norm };
        let s1 = s1 * inv;
        let s2 = s2 * inv;
        let s3 = s3 * inv;
        let s4 = s4 * inv;

        let q_dot1 = 0.5 * (-q2 * gx - q3 * gy - q4 * gz) - self.beta * s1;
        let q_dot2 = 0.5 * (q1 * gx + q3 * gz - q4 * gy) - self.beta * s2;
        let q_dot3 = 0.5 * (q1 * gy - q2 * gz + q4 * gx) - self.beta * s3;
        let q_dot4 = 0.5 * (q1 * gz + q2 * gy - q3 * gx) - self.beta * s4;

        q1 += q_dot1 * self.sample_period;
        q2 += q_dot2 * self.sample_period;
        q3 += q_dot3 * self.sample_period;
        q4 += q_dot4 * self.sample_period;

        let qn = (q1 * q1 + q2 * q2 + q3 * q3 + q4 * q4).sqrt();
        if qn == 0.0 {
            return;
        }
        let inv = 1.0 / qn;
        self.quat = [q1 * inv, q2 * inv, q3 * inv, q4 * inv];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_to_identity() {
        let m = Madgwick::new(1.0 / 200.0);
        assert_eq!(m.quaternion(), [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn default_beta_is_one() {
        let m = Madgwick::new(1.0 / 200.0);
        assert_eq!(m.beta, 1.0);
    }
}
