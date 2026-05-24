//! `DualSenseFactory` — `DeviceFactory` impl with one-shot pairing scan helper.

use crate::device::DualSenseDevice;
use crate::hid::hid_api_singleton;
use crate::ids::{ControllerKind, SONY_VID};
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct DualSenseFactory {
    mode: FactoryMode,
}

enum FactoryMode {
    Real,
    #[cfg(feature = "synthetic-source")]
    Synthetic {
        count: u8,
    },
}

impl DualSenseFactory {
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

impl Default for DualSenseFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DeviceFactory for DualSenseFactory {
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

impl DualSenseFactory {
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
                    .filter(|i| {
                        let up = i.usage_page();
                        let u = i.usage();
                        up == 0 && u == 0 || (up == 0x01 && u == 0x05)
                    })
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

            // Drop devices that have left the HID bus so a DualSense which
            // disconnects mid-session (USB unplug, BT drop) is re-emitted as
            // a fresh device the moment it reappears. Without this prune
            // the entry sticks in `known` forever and auto-reconnect needs
            // an app restart — same fix already in device-joycon.
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
                            tracing::warn!(?path, error = %e, "failed to open dualsense");
                            known.remove(&key);
                            continue;
                        }
                    }
                };

                let mac = mac_from_serial(&serial);
                let dev = DualSenseDevice::new(device, kind, serial, mac);
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
            let dev = crate::synthetic::SyntheticDualSense::new(i as u64);
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
pub struct PairedDualSense {
    pub kind: ControllerKind,
    pub pid: u16,
    pub serial: String,
    pub path: String,
    pub interface: i32,
    pub mac: [u8; 6],
}

impl DualSenseFactory {
    pub fn list_paired() -> Result<Vec<PairedDualSense>, DeviceError> {
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
            let up = i.usage_page();
            let u = i.usage();
            let usage_ok = (up == 0 && u == 0) || (up == 0x01 && u == 0x05);
            if !usage_ok {
                continue;
            }
            let pid_value = i.product_id();
            let kind = match ControllerKind::from_pid(pid_value) {
                Some(k) => k,
                None => continue,
            };
            let serial = i.serial_number().unwrap_or("").to_string();
            let mac = mac_from_serial(&serial);
            out.push(PairedDualSense {
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

/// Deterministic locally-administered MAC derived from the controller's
/// USB serial. FNV-1a so the same serial maps to the same MAC across
/// app restarts and Rust toolchain versions — the per-device settings
/// store keys off MAC and silently loses settings if this drifts.
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

#[cfg(test)]
mod tests {
    use super::mac_from_serial;

    #[test]
    fn mac_is_deterministic_across_calls() {
        let a = mac_from_serial("AA:BB:CC:DD:EE:FF");
        let b = mac_from_serial("AA:BB:CC:DD:EE:FF");
        assert_eq!(a, b);
    }

    #[test]
    fn mac_is_locally_administered() {
        let m = mac_from_serial("AA:BB:CC:DD:EE:FF");
        assert_eq!(m[0] & 0x02, 0x02, "locally-administered bit must be set");
        assert_eq!(m[0] & 0x01, 0x00, "must not be multicast");
    }

    #[test]
    fn distinct_serials_yield_distinct_macs() {
        let a = mac_from_serial("AA:BB:CC:DD:EE:FF");
        let b = mac_from_serial("11:22:33:44:55:66");
        assert_ne!(a, b);
    }

    #[test]
    fn fnv1a_matches_reference_vector() {
        // Reference FNV-1a("foobar") = 0x85944171f73967e8 (known test vector)
        assert_eq!(super::fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }
}
