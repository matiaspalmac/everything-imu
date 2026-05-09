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

                let mac = mac_from_serial(&serial);
                let dev = PsMoveDevice::new(device, kind, serial, mac);
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

fn mac_from_serial(serial: &str) -> [u8; 6] {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    serial.hash(&mut hasher);
    let h = hasher.finish().to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}
