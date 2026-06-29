//! `BiasStore` impl backed by SQLite `bias_seeds` table.

use crate::db::PersistenceDb;
use device_traits::{BiasStore, DeviceId};
use rusqlite::{params, OptionalExtension};
use std::sync::Arc;

pub struct SqliteBiasStore {
    db: Arc<PersistenceDb>,
}

impl SqliteBiasStore {
    pub fn new(db: Arc<PersistenceDb>) -> Self {
        Self { db }
    }
}

impl BiasStore for SqliteBiasStore {
    fn load_bias(&self, id: &DeviceId) -> Option<[f64; 3]> {
        let conn = self.db.conn.lock().unwrap_or_else(|e| e.into_inner());
        // optional() distinguishes "no row" from a real SQLite error; the
        // latter is worth logging instead of silently treating as None.
        let row = conn
            .query_row(
                "SELECT bias_x, bias_y, bias_z FROM bias_seeds WHERE mac = ?1",
                params![id.mac.as_slice()],
                |r| {
                    Ok([
                        r.get::<_, f64>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, f64>(2)?,
                    ])
                },
            )
            .optional();
        match row {
            Ok(Some(v)) => Some(v),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(error = %e, "load_bias query failed");
                None
            }
        }
    }

    fn store_bias(&self, id: &DeviceId, bias: [f64; 3]) {
        // Defense in depth: the SettingsStore/BiasStore contract requires
        // non-finite values to be rejected by every implementation. A NaN
        // bias persisted here would re-seed the fusion filter into a
        // permanent NaN state on next start.
        if !bias.iter().all(|v| v.is_finite()) {
            tracing::warn!(
                bias = ?bias,
                "refusing to persist non-finite bias (contract violation upstream)"
            );
            return;
        }
        let conn = self.db.conn.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = conn.execute(
            "INSERT INTO bias_seeds (mac, serial, bias_x, bias_y, bias_z) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(mac) DO UPDATE SET
                serial = excluded.serial,
                bias_x = excluded.bias_x,
                bias_y = excluded.bias_y,
                bias_z = excluded.bias_z,
                updated = unixepoch()",
            params![id.mac.as_slice(), id.serial, bias[0], bias[1], bias[2]],
        ) {
            tracing::warn!(error = %e, "failed to persist bias seed");
        }
    }
}
