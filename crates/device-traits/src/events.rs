//! Per-device event taxonomy.

use crate::device::DeviceId;
use crate::reset::ResetKind;

#[derive(Debug, Clone, Copy)]
pub struct ImuSample {
    /// Gyroscope in rad/s, body frame.
    pub gyro: [f32; 3],
    /// Accelerometer in m/s², body frame.
    pub accel: [f32; 3],
    /// Optional magnetometer in µT.
    pub mag: Option<[f32; 3]>,
    /// Monotonic timestamp from device or capture (microseconds since arbitrary epoch).
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct BatteryState {
    /// Charge fraction 0.0..=1.0 (or NaN if unknown).
    pub fraction: f32,
    pub charging: bool,
}

#[derive(Debug, Clone)]
pub enum ChannelInfo {
    Connected(DeviceId),
    /// Burst of N samples (most controllers ship 3 IMU samples per HID report).
    ImuSamples(Vec<ImuSample>),
    Battery(BatteryState),
    ResetRequested(ResetKind),
    Disconnected,
}
