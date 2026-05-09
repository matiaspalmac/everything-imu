//! `BiasStore` impl backed by SQLite `bias_seeds` table.

use crate::db::PersistenceDb;
use device_traits::{BiasStore, DeviceId};
use rusqlite::params;
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
        let conn = self.db.conn.lock().unwrap();
        conn.query_row(
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
        .ok()
    }

    fn store_bias(&self, id: &DeviceId, bias: [f64; 3]) {
        let conn = self.db.conn.lock().unwrap();
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
