//! Valve Steam Controller (wired + wireless dongle) IMU bridge.
//!
//! Hardware: MPU-6500 IMU (accel + gyro) onboard a Steam Controller
//! (discontinued 2019). Two USB enumerations:
//! - VID `0x28DE`, PID `0x1102` — wired
//! - VID `0x28DE`, PID `0x1142` — wireless dongle (multiplexes up to 4 controllers)
//!
//! BLE is intentionally out of scope for v1 — the BLE transport uses a custom
//! 18-byte segmentation that is not a stock GATT characteristic and warrants
//! its own implementation pass.
//!
//! ## Enabling the IMU stream
//! The default firmware ships the IMU disabled. To enable raw gyro + accel we
//! send a feature report `ID_SET_SETTINGS_VALUES` carrying a single TLV:
//! `SETTING_IMU_MODE = SETTING_GYRO_MODE_SEND_RAW_ACCEL | SETTING_GYRO_MODE_SEND_RAW_GYRO`.
//!
//! ## Scaling
//! Per SDL `SDL_hidapi_steam.c`:
//!   - gyro:  `(raw / 32768) * (2000 * π / 180)` (±2000 dps)
//!   - accel: `(raw / 32768) * 2 * 9.80665` (±2 g)

pub mod device;
pub mod factory;
pub mod ids;
pub mod report;
pub mod scale;
pub mod subcmd;

#[cfg(feature = "synthetic-source")]
pub mod synthetic;

pub use device::SteamControllerDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::SteamControllerFactory;
pub use ids::{PID_DONGLE, PID_WIRED, VALVE_VID};
