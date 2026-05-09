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
        format!(
            "rotation_offset_deg:{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            id.mac[0], id.mac[1], id.mac[2], id.mac[3], id.mac[4], id.mac[5]
        )
    }
}

impl SettingsStore for SqliteSettingsStore {
    fn get_rotation_offset_deg(&self, id: &DeviceId) -> f32 {
        let key = Self::key_for(id);
        self.db
            .get_setting(&key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    }

    fn set_rotation_offset_deg(&self, id: &DeviceId, deg: f32) {
        let key = Self::key_for(id);
        if let Err(e) = self.db.set_setting(&key, &deg.to_string()) {
            tracing::warn!(error = %e, key = %key, "failed to persist rotation offset");
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.db.get_setting(key).ok().flatten()
    }

    fn set(&self, key: &str, value: &str) {
        if let Err(e) = self.db.set_setting(key, value) {
            tracing::warn!(error = %e, key = %key, "failed to persist setting");
        }
    }
}
