//! `DualShock3Factory` — polls hidapi for connected SIXAXIS / DS3 pads.

use crate::device::{hid_api_singleton, DualShock3Device};
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

const SONY_VID: u16 = 0x054C;
const DS3_PID: u16 = 0x0268;

#[derive(Default, Clone)]
pub struct DualShock3Factory;

impl DualShock3Factory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DeviceFactory for DualShock3Factory {
    async fn enumerate_loop(
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
                    .filter(|i| i.vendor_id() == SONY_VID && i.product_id() == DS3_PID)
                    .map(|i| {
                        (
                            i.path().to_owned(),
                            i.serial_number().unwrap_or("").to_string(),
                            i.interface_number(),
                        )
                    })
                    .collect()
            };

            // Prune departed pads so an unplug/replug re-registers cleanly.
            let present: HashSet<String> = infos
                .iter()
                .map(|(path, _, iface)| format!("{path:?}#{iface}"))
                .collect();
            known.retain(|k| present.contains(k));

            for (path, serial, iface) in infos {
                let key = format!("{path:?}#{iface}");
                if !known.insert(key.clone()) {
                    continue;
                }
                let device = {
                    let guard = api.lock().unwrap();
                    match guard.open_path(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!(?path, error = %e, "failed to open ds3");
                            known.remove(&key);
                            continue;
                        }
                    }
                };
                let mac = mac_from_serial(&serial);
                let dev = DualShock3Device::new(device, serial, mac);
                let meta = dev.metadata().clone();
                if out.send((meta, Box::new(dev))).await.is_err() {
                    return Ok(());
                }
            }

            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
    }
}

/// Deterministic locally-administered MAC from the pad serial (FNV-1a). Falls
/// back to the path-less constant set when the serial is empty — DS3s often
/// report no serial, so distinct pads on the same host may collide; acceptable
/// for an experimental driver.
fn mac_from_serial(serial: &str) -> [u8; 6] {
    let h = fnv1a_64(serial.as_bytes()).to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x00000100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_is_locally_administered_and_deterministic() {
        let a = mac_from_serial("DS3-001");
        let b = mac_from_serial("DS3-001");
        assert_eq!(a, b);
        assert_eq!(a[0] & 0x02, 0x02);
        assert_eq!(a[0] & 0x01, 0x00);
    }
}
