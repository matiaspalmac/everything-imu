//! VRChat OSC haptic bridge — DTOs, Tauri commands, and boot wiring.
//!
//! The bridge itself lives in the `osc-haptics` crate. This module persists
//! its config in the settings DB, exposes get/set commands to the UI, and
//! forwards discovered OSC addresses as Tauri events.

use crate::error::IpcError;
use crate::events::HapticAddressDiscovered;
use crate::state::AppHandle;
use everything_imu_core::AppState;
use osc_haptics::{HapticConfig, HapticMode, HapticRule, RumbleSink};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle as TauriAppHandle, State};
use tauri_specta::Event;
use tokio::sync::{mpsc, watch};

/// DB key holding the haptic config as a JSON blob.
const CONFIG_KEY: &str = "haptic_config";

// --- DTOs (frontend-facing, MACs as hex strings) ---------------------------

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HapticModeDto {
    Proximity { gain: f32, min_threshold: f32 },
    Pulse { pulse_ms: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct HapticRuleDto {
    pub osc_address: String,
    /// Target device MAC as 12 lowercase hex chars (no separators).
    pub device_mac: String,
    pub mode: HapticModeDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct HapticConfigDto {
    pub enabled: bool,
    pub listen_port: u16,
    pub rules: Vec<HapticRuleDto>,
}

// --- DTO <-> domain conversions --------------------------------------------

fn mac_to_hex(mac: [u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse 12 hex chars into a MAC. Returns `None` on bad length or digits.
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    if s.len() != 12 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(mac)
}

impl From<HapticMode> for HapticModeDto {
    fn from(m: HapticMode) -> Self {
        match m {
            HapticMode::Proximity {
                gain,
                min_threshold,
            } => HapticModeDto::Proximity {
                gain,
                min_threshold,
            },
            HapticMode::Pulse { pulse_ms } => HapticModeDto::Pulse { pulse_ms },
        }
    }
}

impl From<HapticModeDto> for HapticMode {
    fn from(m: HapticModeDto) -> Self {
        match m {
            HapticModeDto::Proximity {
                gain,
                min_threshold,
            } => HapticMode::Proximity {
                gain,
                min_threshold,
            },
            HapticModeDto::Pulse { pulse_ms } => HapticMode::Pulse { pulse_ms },
        }
    }
}

fn config_to_dto(c: HapticConfig) -> HapticConfigDto {
    HapticConfigDto {
        enabled: c.enabled,
        listen_port: c.listen_port,
        rules: c
            .rules
            .into_iter()
            .map(|r| HapticRuleDto {
                osc_address: r.osc_address,
                device_mac: mac_to_hex(r.device_mac),
                mode: r.mode.into(),
            })
            .collect(),
    }
}

/// Convert a UI config into the domain type. Rules with an unparseable MAC
/// are dropped rather than failing the whole save.
fn dto_to_config(d: HapticConfigDto) -> HapticConfig {
    HapticConfig {
        enabled: d.enabled,
        listen_port: d.listen_port,
        rules: d
            .rules
            .into_iter()
            .filter_map(|r| {
                Some(HapticRule {
                    osc_address: r.osc_address,
                    device_mac: parse_mac(&r.device_mac)?,
                    mode: r.mode.into(),
                })
            })
            .collect(),
    }
}

// --- persistence -----------------------------------------------------------

/// Load the persisted haptic config, falling back to the default if absent
/// or corrupt.
pub fn load_config(db: &persistence::PersistenceDb) -> HapticConfig {
    db.get_setting(CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

// --- Tauri commands --------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_haptic_config(handle: State<'_, AppHandle>) -> Result<HapticConfigDto, IpcError> {
    Ok(config_to_dto(load_config(&handle.db)))
}

#[tauri::command]
#[specta::specta]
pub async fn set_haptic_config(
    handle: State<'_, AppHandle>,
    config: HapticConfigDto,
) -> Result<(), IpcError> {
    let domain = dto_to_config(config);
    let json = serde_json::to_string(&domain).map_err(|e| IpcError::Internal(e.to_string()))?;
    handle.db.set_setting(CONFIG_KEY, &json)?;
    // Push to the live bridge — applies without restart. A send error only
    // means the bridge task has exited; the DB still holds the new config.
    let _ = handle.haptic_config_tx.send(domain);
    Ok(())
}

// --- boot ------------------------------------------------------------------

/// Spawn the haptic bridge and the discovery-event forwarder.
///
/// Returns the config sender so the caller can stash it in [`AppHandle`].
pub fn spawn(
    app: &TauriAppHandle,
    state: Arc<AppState>,
    initial: HapticConfig,
) -> watch::Sender<HapticConfig> {
    let (config_tx, config_rx) = watch::channel(initial);
    let (discovery_tx, mut discovery_rx) = mpsc::channel::<String>(64);

    let sink: Arc<dyn RumbleSink> = state;
    tauri::async_runtime::spawn(osc_haptics::run_bridge(config_rx, sink, Some(discovery_tx)));

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(address) = discovery_rx.recv().await {
            let _ = HapticAddressDiscovered { address }.emit(&app);
        }
    });

    config_tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_hex_round_trip() {
        let mac = [0x02, 0x57, 0x45, 0xAB, 0xCD, 0xEF];
        let hex = mac_to_hex(mac);
        assert_eq!(hex, "025745abcdef");
        assert_eq!(parse_mac(&hex), Some(mac));
    }

    #[test]
    fn parse_mac_rejects_bad_input() {
        assert_eq!(parse_mac("abc"), None);
        assert_eq!(parse_mac("zzzzzzzzzzzz"), None);
    }

    #[test]
    fn dto_drops_rules_with_bad_mac() {
        let dto = HapticConfigDto {
            enabled: true,
            listen_port: 9001,
            rules: vec![
                HapticRuleDto {
                    osc_address: "/ok".into(),
                    device_mac: "025745abcdef".into(),
                    mode: HapticModeDto::Pulse { pulse_ms: 100 },
                },
                HapticRuleDto {
                    osc_address: "/bad".into(),
                    device_mac: "nope".into(),
                    mode: HapticModeDto::Pulse { pulse_ms: 100 },
                },
            ],
        };
        let domain = dto_to_config(dto);
        assert_eq!(domain.rules.len(), 1);
        assert_eq!(domain.rules[0].osc_address, "/ok");
    }
}
