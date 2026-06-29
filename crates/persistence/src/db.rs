//! Connection wrapper + migrations + extension API.

use crate::error::PersistenceError;
use crate::history::DeviceHistoryRow;
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(vec![M::up(include_str!("../migrations/001_init.sql"))]));

pub struct PersistenceDb {
    pub(crate) conn: Mutex<Connection>,
}

impl PersistenceDb {
    pub fn open(path: &Path) -> Result<Self, PersistenceError> {
        let mut conn = Connection::open(path)?;
        Self::tune(&conn)?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, PersistenceError> {
        let mut conn = Connection::open_in_memory()?;
        Self::tune(&conn)?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn tune(conn: &Connection) -> Result<(), PersistenceError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, PersistenceError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // optional() distinguishes "no such row" (legitimate None) from
        // an underlying SQLite error (worth surfacing to the caller).
        let v: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated = unixepoch()",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn list_device_history(&self) -> Result<Vec<DeviceHistoryRow>, PersistenceError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT mac, serial, kind, last_seen, rotation_deg FROM device_history ORDER BY last_seen DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let mac_blob: Vec<u8> = r.get(0)?;
            // A non-6-byte MAC blob is a corrupted row. Skip it rather than
            // zero-padding/truncating into a phantom device with a near-null
            // identity. Surface it as a warning so the cause is visible.
            if mac_blob.len() != 6 {
                tracing::warn!(
                    actual_len = mac_blob.len(),
                    "device_history row has malformed mac blob; skipping row"
                );
                return Ok(None);
            }
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&mac_blob);
            Ok(Some(DeviceHistoryRow {
                mac,
                serial: r.get(1)?,
                kind: r.get(2)?,
                last_seen: r.get(3)?,
                rotation_deg: r.get(4)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            if let Some(r) = row? {
                out.push(r);
            }
        }
        Ok(out)
    }

    pub fn upsert_device_history(
        &self,
        mac: [u8; 6],
        serial: &str,
        kind: &str,
        last_seen: i64,
        rotation_deg: f32,
    ) -> Result<(), PersistenceError> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO device_history (mac, serial, kind, last_seen, rotation_deg)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(mac) DO UPDATE SET
                serial = excluded.serial,
                kind = excluded.kind,
                last_seen = excluded.last_seen,
                rotation_deg = excluded.rotation_deg",
            params![mac.as_slice(), serial, kind, last_seen, rotation_deg],
        )?;
        Ok(())
    }
}
