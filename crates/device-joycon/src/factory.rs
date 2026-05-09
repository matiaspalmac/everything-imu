//! `JoyconFactory` — `DeviceFactory` impl.

use crate::hid::hid_api_singleton;
use crate::ids::{pid, ControllerKind, JOYCON_VID};
use crate::jc1::JoyCon1Device;
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct JoyconFactory {
    mode: FactoryMode,
}

enum FactoryMode {
    Real,
    #[cfg(feature = "synthetic-source")]
    Synthetic {
        count: u8,
    },
}

impl JoyconFactory {
    pub fn real() -> Self {
        Self {
            mode: FactoryMode::Real,
        }
    }

    #[cfg(feature = "synthetic-source")]
    pub fn synthetic(count: u8) -> Self {
        Self {
            mode: FactoryMode::Synthetic { count },
        }
    }
}

#[async_trait::async_trait]
impl DeviceFactory for JoyconFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        match &self.mode {
            FactoryMode::Real => self.real_enumerate_loop(out).await,
            #[cfg(feature = "synthetic-source")]
            FactoryMode::Synthetic { count } => self.synthetic_enumerate_loop(*count, out).await,
        }
    }
}

impl JoyconFactory {
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
                    .filter(|i| i.vendor_id() == JOYCON_VID)
                    .filter(|i| {
                        let up = i.usage_page();
                        let u = i.usage();
                        if up == 0 && u == 0 {
                            return true;
                        }
                        up == 0x01 && u == 0x05
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

            for (path, pid_value, serial, iface) in infos {
                let key = format!("{path:?}#{iface}");
                if !known.insert(key.clone()) {
                    continue;
                }
                let kind_from_pid = ControllerKind::from_pid(pid_value);
                let kind = match (pid_value, kind_from_pid) {
                    (_, Some(k)) => k,
                    (pid::CHARGING_GRIP, None) => {
                        tracing::warn!(
                            ?path,
                            "charging grip detected — Sprint 3 skips, use BT pairing"
                        );
                        continue;
                    }
                    _ => continue,
                };

                let device = {
                    let guard = api.lock().unwrap();
                    match guard.open_path(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!(?path, error = %e, "failed to open hid device");
                            known.remove(&key);
                            continue;
                        }
                    }
                };

                let mac = mac_from_string(&format!("{serial}{path:?}"));
                let is_usb = iface >= 0;
                let dev = JoyCon1Device::new(device, kind, is_usb, serial.clone(), mac);
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
    async fn synthetic_enumerate_loop(
        &self,
        count: u8,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        for i in 0..count {
            let dev = crate::synthetic::SyntheticJoyConL::new(i as u64);
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

/// One-shot paired Joy-Con scan result, returned by [`JoyconFactory::list_paired`].
///
/// Used by tooling (`headless-cli --list-devices`) to enumerate visible
/// controllers without starting the tracking pipeline.
#[derive(Debug, Clone)]
pub struct PairedJoycon {
    pub kind: ControllerKind,
    pub pid: u16,
    pub serial: String,
    pub path: String,
    pub interface: i32,
    pub mac: [u8; 6],
}

impl JoyconFactory {
    /// Synchronously enumerate currently visible Joy-Con / Pro Controller HID
    /// devices. Filters by Nintendo VID + game-pad usage page (matches the
    /// real enumerate loop).
    ///
    /// Returns an empty vec if hidapi is reachable but nothing is paired.
    pub fn list_paired() -> Result<Vec<PairedJoycon>, DeviceError> {
        let api = hid_api_singleton().map_err(|e| DeviceError::Hid(e.to_string()))?;
        let mut guard = api.lock().unwrap();
        guard
            .refresh_devices()
            .map_err(|e| DeviceError::Hid(e.to_string()))?;
        let mut out = Vec::new();
        for i in guard.device_list() {
            if i.vendor_id() != JOYCON_VID {
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
            let mac = mac_from_string(&format!("{serial}{:?}", i.path()));
            out.push(PairedJoycon {
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

fn mac_from_string(s: &str) -> [u8; 6] {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    s.hash(&mut hasher);
    let h = hasher.finish().to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}
