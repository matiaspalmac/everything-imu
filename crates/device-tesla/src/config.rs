//! User-supplied Tesla connection config.
//!
//! Two flavours:
//! - [`TeslaConfig::Live`] — real Fleet API credentials. The user obtains a
//!   refresh token via the public Tesla SSO flow (out-of-band) and pastes it
//!   here; we never touch their password.
//! - [`TeslaConfig::Synthetic`] — replay a recorded drive trace so the
//!   pipeline can be exercised on a developer machine without a vehicle.

use std::path::PathBuf;
use std::time::Duration;

/// Fleet API region. Determines which auth + API host we hit.
///
/// Tesla split the Fleet API into regional silos in 2024. The auth host is
/// always `auth.tesla.com`, but vehicle endpoints live on
/// `fleet-api.prd.{na,eu,cn}.vn.cloud.tesla.com`. We default to NA because
/// that's where the streaming WS host used to live; users outside NA must
/// override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Region {
    #[default]
    NorthAmerica,
    Europe,
    China,
}

impl Region {
    /// Base URL for vehicle endpoints, no trailing slash.
    pub fn api_host(self) -> &'static str {
        match self {
            Self::NorthAmerica => "https://fleet-api.prd.na.vn.cloud.tesla.com",
            Self::Europe => "https://fleet-api.prd.eu.vn.cloud.tesla.com",
            Self::China => "https://fleet-api.prd.cn.vn.cloud.tesla.cn",
        }
    }

    /// WebSocket streaming endpoint.
    pub fn streaming_url(self) -> &'static str {
        // Tesla retained a single global streaming endpoint as of 2025-Q1.
        // The region split is REST-only.
        "wss://streaming.vn.teslamotors.com/streaming/"
    }
}

/// Live Fleet API credentials.
#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// OAuth2 refresh token issued by Tesla SSO (`auth.tesla.com`). Long-lived.
    pub refresh_token: String,
    /// Fleet API client ID registered with Tesla (developer console).
    pub client_id: String,
    /// Vehicle ID (not VIN). Numeric ID returned by `/api/1/vehicles`.
    pub vehicle_id: u64,
    /// Stable MAC bytes used to identify this tracker to SlimeVR-Server.
    /// We synthesise from the vehicle ID so reconnects keep the same identity.
    pub vehicle_vin_tail: [u8; 6],
    /// Regional Fleet API silo.
    pub region: Region,
    /// Streaming idle timeout. Tesla closes the socket if the vehicle goes
    /// to sleep; we reconnect with backoff.
    pub idle_timeout: Duration,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            refresh_token: String::new(),
            client_id: String::new(),
            vehicle_id: 0,
            vehicle_vin_tail: [0; 6],
            region: Region::default(),
            idle_timeout: Duration::from_secs(60),
        }
    }
}

/// Synthetic replay config — no network, deterministic fixture data.
#[derive(Debug, Clone)]
pub struct SyntheticConfig {
    /// Optional path to a JSON fixture (heading + speed time series). If
    /// `None`, the synthetic device generates a procedural figure-eight drive.
    pub fixture: Option<PathBuf>,
    /// Replay rate in Hz. The synthetic loop sleeps to maintain this cadence.
    pub rate_hz: u16,
    /// MAC bytes assigned to the synthetic tracker.
    pub mac: [u8; 6],
}

impl Default for SyntheticConfig {
    fn default() -> Self {
        Self {
            fixture: None,
            rate_hz: 10,
            mac: *b"TESLA1",
        }
    }
}

#[derive(Debug, Clone)]
pub enum TeslaConfig {
    Live(LiveConfig),
    Synthetic(SyntheticConfig),
}

impl TeslaConfig {
    /// Read live config from environment variables — the path the CLI uses
    /// when the user has set `TESLA_REFRESH_TOKEN` and friends. Returns
    /// `None` when any required variable is missing so callers can fall back
    /// to disabled mode silently.
    pub fn from_env() -> Option<Self> {
        let refresh_token = std::env::var("TESLA_REFRESH_TOKEN").ok()?;
        let client_id = std::env::var("TESLA_CLIENT_ID").ok()?;
        let vehicle_id: u64 = std::env::var("TESLA_VEHICLE_ID").ok()?.parse().ok()?;
        let region = match std::env::var("TESLA_REGION").as_deref() {
            Ok("eu") => Region::Europe,
            Ok("cn") => Region::China,
            _ => Region::NorthAmerica,
        };
        // Derive stable MAC bytes from vehicle_id so reconnects keep identity.
        let id = vehicle_id.to_be_bytes();
        let vin_tail = [id[2], id[3], id[4], id[5], id[6], id[7]];
        Some(Self::Live(LiveConfig {
            refresh_token,
            client_id,
            vehicle_id,
            vehicle_vin_tail: vin_tail,
            region,
            idle_timeout: Duration::from_secs(60),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_hosts_distinct_per_silo() {
        assert_ne!(Region::NorthAmerica.api_host(), Region::Europe.api_host());
        assert_ne!(Region::Europe.api_host(), Region::China.api_host());
    }

    #[test]
    fn from_env_returns_none_when_unset() {
        // Snapshot + clear the variables we care about so this test is
        // hermetic regardless of the developer's shell.
        let keys = [
            "TESLA_REFRESH_TOKEN",
            "TESLA_CLIENT_ID",
            "TESLA_VEHICLE_ID",
        ];
        let snapshot: Vec<(&str, Option<String>)> =
            keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();
        for (k, _) in &snapshot {
            // SAFETY: tests in this crate run single-threaded under cargo's
            // default settings; we restore the prior value before returning.
            unsafe {
                std::env::remove_var(k);
            }
        }
        let res = TeslaConfig::from_env();
        for (k, v) in snapshot {
            // SAFETY: see above.
            unsafe {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
        assert!(res.is_none(), "should return None with required vars unset");
    }
}
