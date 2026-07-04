//! [`device_traits::DeviceFactory`] that discovers trackers by advertised name.
//!
//! Every unit advertises a shared local-name prefix (see
//! [`crate::protocol::NAME_PREFIX`]); the MAC and per-unit serial vary, so the
//! advertised name — not a hard-coded address — is the discovery key. The
//! enumerate loop scans every adapter, emitting a [`HopxDevice`] the first time
//! it sees each matching peripheral.

use std::collections::HashMap;
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::Manager;
use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use tokio::sync::mpsc;

use crate::device::HopxDevice;
use crate::protocol;

const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct HopxFactory;

impl HopxFactory {
    pub fn new() -> Self {
        Self
    }
}

/// A tracker seen during a one-shot scan — surfaced to the UI device picker.
#[derive(Debug, Clone)]
pub struct NearbyHopx {
    pub name: String,
    pub address: String,
    pub mac: [u8; 6],
}

#[async_trait::async_trait]
impl DeviceFactory for HopxFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let manager = Manager::new()
            .await
            .map_err(|e| DeviceError::Hid(format!("hopx manager init failed: {e}")))?;
        let adapters = manager
            .adapters()
            .await
            .map_err(|e| DeviceError::Hid(format!("hopx adapters query failed: {e}")))?;
        if adapters.is_empty() {
            // No BLE adapter — nothing to enumerate. Exit cleanly so the
            // supervisor's other factories are unaffected.
            return Ok(());
        }
        for adapter in &adapters {
            if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                tracing::warn!(error = %e, "hopx start_scan failed; skipping adapter");
            }
        }

        // Addresses already emitted this session. A unit is emitted once; if it
        // later disconnects the user re-triggers discovery, matching the other
        // BLE drivers' one-shot emit behaviour.
        let mut known: HashMap<String, ()> = HashMap::new();
        loop {
            if out.is_closed() {
                return Ok(());
            }
            for adapter in &adapters {
                let peripherals = match adapter.peripherals().await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, "hopx peripherals query failed");
                        continue;
                    }
                };
                for peripheral in peripherals {
                    let Some(props) = peripheral.properties().await.ok().flatten() else {
                        continue;
                    };
                    let Some(name) = props.local_name.as_deref() else {
                        continue;
                    };
                    if !protocol::name_matches(name) {
                        continue;
                    }
                    let addr = props.address.to_string();
                    if known.contains_key(&addr) {
                        continue;
                    }
                    if peripheral.is_connected().await.unwrap_or(false) {
                        known.insert(addr, ());
                        continue;
                    }
                    known.insert(addr.clone(), ());

                    let mac = mac_from_addr(&addr).unwrap_or_else(|| hash_to_mac(&addr));
                    let serial = protocol::serial_from_name(name).unwrap_or_else(|| addr.clone());
                    let dev = HopxDevice::new(peripheral, serial, mac);
                    let meta = dev.metadata().clone();
                    if out
                        .send((meta, Box::new(dev) as Box<dyn Device>))
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(RESCAN_INTERVAL) => {}
                _ = out.closed() => return Ok(()),
            }
        }
    }
}

/// One-shot scan returning matching trackers — for the UI "scan" button.
pub async fn scan_nearby(timeout: Duration) -> Result<Vec<NearbyHopx>, DeviceError> {
    let manager = Manager::new()
        .await
        .map_err(|e| DeviceError::Hid(format!("hopx manager init failed: {e}")))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|e| DeviceError::Hid(format!("hopx adapters query failed: {e}")))?;
    if adapters.is_empty() {
        return Ok(Vec::new());
    }
    for adapter in &adapters {
        if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
            tracing::warn!(error = %e, "hopx one-shot start_scan failed");
        }
    }
    tokio::time::sleep(timeout).await;

    // One-shot scan: stop the radio before collecting results so we don't leave
    // scanning running after this function returns (covers all exit paths below).
    for adapter in &adapters {
        let _ = adapter.stop_scan().await;
    }

    let mut seen = HashMap::new();
    let mut out = Vec::new();
    for adapter in &adapters {
        let peripherals = adapter.peripherals().await.map_err(|e| {
            DeviceError::Hid(format!("hopx one-shot peripherals query failed: {e}"))
        })?;
        for peripheral in peripherals {
            let Some(props) = peripheral.properties().await.ok().flatten() else {
                continue;
            };
            let Some(name) = props.local_name.as_deref() else {
                continue;
            };
            if !protocol::name_matches(name) {
                continue;
            }
            let address = props.address.to_string();
            if seen.insert(address.clone(), ()).is_some() {
                continue;
            }
            let mac = mac_from_addr(&address).unwrap_or_else(|| hash_to_mac(&address));
            out.push(NearbyHopx {
                name: name.to_string(),
                address,
                mac,
            });
        }
    }
    Ok(out)
}

/// Parse a colon-delimited BLE address (`"E4:CE:DB:06:D6:87"`) into a 6-byte MAC.
fn mac_from_addr(addr: &str) -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut count = 0usize;
    for (idx, part) in addr.split(':').enumerate() {
        if idx >= 6 {
            return None;
        }
        out[idx] = u8::from_str_radix(part, 16).ok()?;
        count += 1;
    }
    (count == 6).then_some(out)
}

/// Stable fallback MAC when a BLE address cannot be parsed. FNV-1a keeps the
/// mapping deterministic across restarts so the per-device settings store keys
/// consistently off MAC.
fn hash_to_mac(seed: &str) -> [u8; 6] {
    let h = fnv1a_64(seed.as_bytes()).to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

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
    use super::*;

    #[test]
    fn parses_ble_address_into_mac() {
        assert_eq!(
            mac_from_addr("E4:CE:DB:06:D6:87"),
            Some([0xE4, 0xCE, 0xDB, 0x06, 0xD6, 0x87])
        );
        assert_eq!(mac_from_addr("E4:CE:DB"), None);
    }

    #[test]
    fn hash_to_mac_is_deterministic_and_locally_administered() {
        let a = hash_to_mac("Triki 257739387");
        let b = hash_to_mac("Triki 257739387");
        assert_eq!(a, b);
        // Locally-administered bit set, not a real OUI.
        assert_eq!(a[0], 0x02);
    }
}
