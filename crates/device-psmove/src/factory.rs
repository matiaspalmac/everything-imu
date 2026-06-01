//! `PsMoveFactory` — `DeviceFactory` impl + one-shot scan helper.

use crate::device::PsMoveDevice;
use crate::hid::hid_api_singleton;
use crate::ids::{ControllerKind, SONY_VID};
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct PsMoveFactory {
    mode: FactoryMode,
}

enum FactoryMode {
    Real,
    #[cfg(feature = "synthetic-source")]
    Synthetic {
        count: u8,
    },
}

impl PsMoveFactory {
    pub fn new() -> Self {
        Self {
            mode: FactoryMode::Real,
        }
    }

    pub fn real() -> Self {
        Self::new()
    }

    #[cfg(feature = "synthetic-source")]
    pub fn synthetic(count: u8) -> Self {
        Self {
            mode: FactoryMode::Synthetic { count },
        }
    }
}

impl Default for PsMoveFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DeviceFactory for PsMoveFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        match &self.mode {
            FactoryMode::Real => self.real_enumerate_loop(out).await,
            #[cfg(feature = "synthetic-source")]
            FactoryMode::Synthetic { count } => self.synth_enumerate_loop(*count, out).await,
        }
    }
}

impl PsMoveFactory {
    async fn real_enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let api = hid_api_singleton().map_err(|e| DeviceError::Hid(e.to_string()))?;
        let mut known: HashSet<String> = HashSet::new();

        loop {
            {
                let mut guard = api.lock().unwrap();
                guard
                    .refresh_devices()
                    .map_err(|e| DeviceError::Hid(e.to_string()))?;
            }

            let infos: Vec<_> = {
                let guard = api.lock().unwrap();
                guard
                    .device_list()
                    .filter(|i| i.vendor_id() == SONY_VID)
                    .filter(|i| ControllerKind::from_pid(i.product_id()).is_some())
                    .map(|i| {
                        (
                            i.path().to_owned(),
                            i.product_id(),
                            i.serial_number().unwrap_or("").to_string(),
                            i.interface_number(),
                        )
                    })
                    .collect()
            };

            // Forget devices no longer present on the HID bus so a PS Move
            // unplugged mid-session is re-emitted as soon as it reappears.
            // Without this, `known` retains the path forever and auto-
            // reconnect requires an app restart. Aligned with joycon/dualsense.
            let present: HashSet<String> = infos
                .iter()
                .map(|(path, _, _, iface)| format!("{path:?}#{iface}"))
                .collect();
            known.retain(|key| present.contains(key));

            for (path, pid_value, serial, iface) in infos {
                let key = format!("{path:?}#{iface}");
                if !known.insert(key.clone()) {
                    continue;
                }
                let kind = match ControllerKind::from_pid(pid_value) {
                    Some(k) => k,
                    None => continue,
                };
                let device = {
                    let guard = api.lock().unwrap();
                    match guard.open_path(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!(?path, error = %e, "failed to open psmove");
                            known.remove(&key);
                            continue;
                        }
                    }
                };

                // Factory IMU calibration (feature 0x10) is USB-only; over BT it
                // fails fast and we fall back to identity (VQF warm-up covers the
                // residual bias). See `crate::pairing::read_factory_calibration`.
                let cal = crate::pairing::read_factory_calibration(&device, kind)
                    .unwrap_or_else(|_| crate::calibration::ImuCalibration::identity());
                let mac = mac_from_serial(&serial);
                let mut dev = PsMoveDevice::new(device, kind, serial, mac);
                dev.set_calibration(cal);
                let meta = dev.metadata().clone();
                if out
                    .send((meta, Box::new(dev) as Box<dyn Device>))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }

            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
    }

    #[cfg(feature = "synthetic-source")]
    async fn synth_enumerate_loop(
        &self,
        count: u8,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        for i in 0..count {
            let dev = crate::synthetic::SyntheticPsMove::new(i as u64);
            let meta = dev.metadata().clone();
            if out
                .send((meta, Box::new(dev) as Box<dyn Device>))
                .await
                .is_err()
            {
                return Ok(());
            }
        }
        std::future::pending().await
    }
}

#[derive(Debug, Clone)]
pub struct PairedPsMove {
    pub kind: ControllerKind,
    pub pid: u16,
    pub serial: String,
    pub path: String,
    pub interface: i32,
    pub mac: [u8; 6],
}

impl PsMoveFactory {
    /// Pair the first USB-tethered PS Move to `host_mac` (feature report 0x05),
    /// returning its device id. The controller must be connected by cable —
    /// feature reports are not reachable over Bluetooth.
    pub fn pair(&self, host_mac: [u8; 6]) -> Result<String, DeviceError> {
        let api = hid_api_singleton().map_err(|e| DeviceError::Hid(e.to_string()))?;
        let mut guard = api.lock().unwrap();
        guard
            .refresh_devices()
            .map_err(|e| DeviceError::Hid(e.to_string()))?;
        for i in guard.device_list() {
            if i.vendor_id() != SONY_VID {
                continue;
            }
            if ControllerKind::from_pid(i.product_id()).is_none() {
                continue;
            }
            let serial = i.serial_number().unwrap_or("").to_string();
            let dev = guard
                .open_path(i.path())
                .map_err(|e| DeviceError::Hid(e.to_string()))?;
            crate::pairing::pair_to_host(&dev, host_mac).map_err(DeviceError::Hid)?;
            return Ok(format!("psmove:{serial}"));
        }
        Err(DeviceError::Hid(
            "no wired PS Move found (connect it by USB to pair)".into(),
        ))
    }

    pub fn list_paired() -> Result<Vec<PairedPsMove>, DeviceError> {
        let api = hid_api_singleton().map_err(|e| DeviceError::Hid(e.to_string()))?;
        let mut guard = api.lock().unwrap();
        guard
            .refresh_devices()
            .map_err(|e| DeviceError::Hid(e.to_string()))?;
        let mut out = Vec::new();
        for i in guard.device_list() {
            if i.vendor_id() != SONY_VID {
                continue;
            }
            let pid_value = i.product_id();
            let kind = match ControllerKind::from_pid(pid_value) {
                Some(k) => k,
                None => continue,
            };
            let serial = i.serial_number().unwrap_or("").to_string();
            let mac = mac_from_serial(&serial);
            out.push(PairedPsMove {
                kind,
                pid: pid_value,
                serial,
                path: format!("{:?}", i.path()),
                interface: i.interface_number(),
                mac,
            });
        }
        Ok(out)
    }
}

/// Deterministic locally-administered MAC derived from the PSMove
/// controller's HID serial. FNV-1a keeps the output stable across
/// app restarts and Rust toolchain versions — the per-device settings
/// store keys off MAC.
fn mac_from_serial(serial: &str) -> [u8; 6] {
    let h = fnv1a_64(serial.as_bytes()).to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
