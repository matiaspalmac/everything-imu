//! `SettingsStore` impl backed by SQLite `settings` table.

use crate::db::PersistenceDb;
use device_traits::{DeviceId, SettingsStore};
use std::sync::Arc;

pub struct SqliteSettingsStore {
    db: Arc<PersistenceDb>,
}

impl SqliteSettingsStore {
    pub fn new(db: Arc<PersistenceDb>) -> Self {
        Self { db }
    }

    fn key_for(id: &DeviceId) -> String {
        // Lower-case hex for parity with the other per-device keys built in
        // `core::pipeline_config_from_settings` (`fusion_algo:<mac>`,
        // `mounting_orientation:<mac>`, etc.). Uniform format makes raw DB
        // inspection straightforward and prevents subtle case-mismatch bugs
        // if another path ever writes the same key.
        format!(
            "rotation_offset_deg:{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            id.mac[0], id.mac[1], id.mac[2], id.mac[3], id.mac[4], id.mac[5]
        )
    }
}

impl SettingsStore for SqliteSettingsStore {
    fn get_rotation_offset_deg(&self, id: &DeviceId) -> f32 {
        let key = Self::key_for(id);
        // Log genuine SQLite errors before falling back to the default so a
        // real failure is observable rather than silently swallowed; a
        // missing row is a legitimate None.
        let raw = match self.db.get_setting(&key) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, key = %key, "failed to read rotation offset");
                None
            }
        };
        raw.and_then(|v| v.parse::<f32>().ok())
            // Filter NaN/inf even on the read path: if a poisoned value
            // ever lands in the DB (external write, older code, manual
            // edit), surface a clean 0.0 instead of propagating NaN into
            // the fusion pipeline.
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    }

    fn set_rotation_offset_deg(&self, id: &DeviceId, deg: f32) {
        // Contract: stores MUST reject non-finite values. A NaN rotation
        // offset reloaded into the pipeline produces a NaN quaternion that
        // poisons every downstream rotation packet.
        if !deg.is_finite() {
            tracing::warn!(deg = deg, "refusing to persist non-finite rotation offset");
            return;
        }
        let key = Self::key_for(id);
        if let Err(e) = self.db.set_setting(&key, &deg.to_string()) {
            tracing::warn!(error = %e, key = %key, "failed to persist rotation offset");
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        match self.db.get_setting(key) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, key = %key, "failed to read setting");
                None
            }
        }
    }

    fn set(&self, key: &str, value: &str) {
        if let Err(e) = self.db.set_setting(key, value) {
            tracing::warn!(error = %e, key = %key, "failed to persist setting");
        }
    }
}
