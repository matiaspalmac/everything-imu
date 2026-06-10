//! Steam controller blacklist check + 1-click fix.
//!
//! Steam Input grabs Joy-Con / Pro Controller HID devices on Windows and
//! presents them to games as Steam virtual controllers — which means our
//! own hidapi enumeration never sees them. The fix Valve documents is to
//! add the relevant VID/PID pairs to `controller_blacklist` inside
//! `<Steam install>/config/config.vdf`. This module reads the VDF,
//! reports whether the blacklist is incomplete for Joy-Con + Pro, and
//! writes the patched config back when the user accepts.
//!
//! Logic adapted from slimevr-wrangler (MIT/Apache-2.0). Author-permitted
//! reuse; see CONTRIBUTING.md for the upstream attribution policy.

#![allow(clippy::result_large_err)]
// keyvalues-parser 0.2 marks the convenience `Vdf::parse` constructor
// deprecated in favour of `parse().map(Vdf::from)`, but the replacement is
// noisier and the upstream API has not been removed. Suppress the lint
// here and revisit when bumping to 0.3+.
#![allow(deprecated)]

use std::{fs, io, path::PathBuf};

use itertools::Itertools;
use keyvalues_parser::Vdf;
use regex::{Captures, Regex};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub enum Device {
    Joycon,
    SwitchPro,
}

impl Device {
    fn ids(self) -> Vec<String> {
        match self {
            Device::Joycon => vec![
                "0x057e/0x2006".into(),
                "0x057e/0x2007".into(),
                "0x057e/0x2008".into(),
            ],
            Device::SwitchPro => vec!["0x057e/0x2009".into()],
        }
    }
}

#[derive(Error, Debug)]
pub enum BlacklistError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("vdf parse: {0}")]
    Parse(#[from] keyvalues_parser::error::Error),
    #[error("steam config has no Software section — not a Steam install")]
    Invalid,
    #[error("could not locate the controller_blacklist line in config.vdf")]
    Regex,
    #[error("blacklist was written but re-read did not contain the new entries")]
    Update,
}

fn check_valid(config: &Vdf) -> Result<(), BlacklistError> {
    config
        .value
        .get_obj()
        .and_then(|o| o.get("Software"))
        .is_some_and(|s| !s.is_empty())
        .then_some(())
        .ok_or(BlacklistError::Invalid)
}

fn get_blacklist<'a>(config: &'a Vdf<'a>) -> Option<&'a str> {
    config
        .value
        .get_obj()?
        .get("controller_blacklist")?
        .first()?
        .get_str()
}

#[cfg(windows)]
fn get_steam_path() -> io::Result<PathBuf> {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    hklm.open_subkey("SOFTWARE\\Wow6432Node\\Valve\\Steam")
        .or_else(|_| hklm.open_subkey("SOFTWARE\\Valve\\Steam"))?
        .get_value::<String, _>("InstallPath")
        .map(PathBuf::from)
}

#[cfg(not(windows))]
fn get_steam_path() -> io::Result<PathBuf> {
    Err(io::Error::from(io::ErrorKind::NotFound))
}

fn get_config_path() -> io::Result<PathBuf> {
    let mut p = get_steam_path()?;
    p.push("config");
    p.push("config.vdf");
    Ok(p)
}

fn read_config() -> io::Result<String> {
    fs::read_to_string(get_config_path()?)
}

fn set_blacklist(raw: &str, config: &Vdf<'_>, new_list: &str) -> Result<String, BlacklistError> {
    let output = match get_blacklist(config) {
        Some(old) => {
            let re = Regex::new(&format!(
                r#"((?i)"controller_blacklist"\s*)"{}""#,
                regex::escape(old)
            ))
            .unwrap();
            if re.find_iter(raw).count() != 1 {
                return Err(BlacklistError::Regex);
            }
            re.replace(raw, |caps: &Captures| {
                format!(r#"{}"{}""#, &caps[1], new_list)
            })
        }
        None => {
            let re = Regex::new(r"(\}\s*)$").unwrap();
            if re.find_iter(raw).count() != 1 {
                return Err(BlacklistError::Regex);
            }
            re.replace(raw, |caps: &Captures| {
                format!(
                    "\t\"controller_blacklist\"\t\t\"{}\"\n{}",
                    new_list, &caps[1]
                )
            })
        }
    };
    Ok(output.into())
}

fn verify(config_text: &str, new_list: &str) -> Result<(), BlacklistError> {
    let config = Vdf::parse(config_text)?;
    if let Some(parsed) = get_blacklist(&config) {
        if parsed == new_list {
            return Ok(());
        }
    }
    Err(BlacklistError::Update)
}

#[derive(Debug, Clone, Default)]
struct Blacklist {
    devices: Vec<String>,
}

impl Blacklist {
    fn has(&self, device: Device) -> bool {
        device.ids().iter().all(|d| self.devices.contains(d))
    }
    fn add(&mut self, device: Device) {
        let combined: Vec<String> = std::mem::take(&mut self.devices)
            .into_iter()
            .chain(device.ids())
            .unique()
            .collect();
        self.devices = combined;
    }
    fn read() -> Result<Self, BlacklistError> {
        let text = read_config()?;
        let config = Vdf::parse(&text)?;
        check_valid(&config)?;
        let devices = get_blacklist(&config)
            .map(|l| {
                l.split(',')
                    .map(str::to_lowercase)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { devices })
    }
    fn save(&self) -> Result<(), BlacklistError> {
        let text = read_config()?;
        let config = Vdf::parse(&text)?;
        check_valid(&config)?;
        let new_list = self.devices.join(",");
        let patched = set_blacklist(&text, &config, &new_list)?;
        verify(&patched, &new_list)?;
        fs::write(get_config_path()?, patched)?;
        Ok(())
    }
}

/// Result surfaced to the UI: which devices need to be blacklisted and a
/// human-readable hint. `needs_fix` drives whether a "Fix" button shows.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SteamBlacklistStatus {
    /// True only when Steam is installed AND at least one tracked device
    /// (Joy-Con or Switch Pro) is missing from `controller_blacklist`.
    pub needs_fix: bool,
    /// True when Steam install was not detected at all — UI should hide
    /// the banner entirely in that case.
    pub steam_not_found: bool,
    /// Human-readable status, populated even when needs_fix is false so the
    /// UI can show "All clear" once the user has fixed the config.
    pub info: String,
}

pub fn check() -> SteamBlacklistStatus {
    match Blacklist::read() {
        Ok(list) => {
            let all = [Device::Joycon, Device::SwitchPro];
            let missing = all.iter().filter(|d| !list.has(**d)).count();
            match missing {
                0 => SteamBlacklistStatus {
                    needs_fix: false,
                    steam_not_found: false,
                    info: "Steam controller blacklist OK.".into(),
                },
                _ => SteamBlacklistStatus {
                    needs_fix: true,
                    steam_not_found: false,
                    info: "Steam Input is currently grabbing Joy-Con / Pro Controllers. \
                           Without a fix, hidapi will never see them. Click Fix to add \
                           the required entries to Steam's controller_blacklist."
                        .into(),
                },
            }
        }
        Err(BlacklistError::Io(_)) => SteamBlacklistStatus {
            needs_fix: false,
            steam_not_found: true,
            info: "Steam not detected — nothing to do.".into(),
        },
        Err(e) => SteamBlacklistStatus {
            needs_fix: false,
            steam_not_found: false,
            info: format!("Steam config check failed: {e}"),
        },
    }
}

pub fn apply_fix() -> Result<(), BlacklistError> {
    let mut list = Blacklist::read()?;
    list.add(Device::Joycon);
    list.add(Device::SwitchPro);
    list.save()
}
