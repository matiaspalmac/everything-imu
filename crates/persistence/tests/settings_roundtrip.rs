use device_traits::{DeviceId, SettingsStore};
use persistence::{PersistenceDb, SqliteSettingsStore};
use std::sync::Arc;

fn id(serial: &str) -> DeviceId {
    DeviceId {
        mac: [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01],
        serial: serial.into(),
    }
}

#[test]
fn default_zero_when_unset() {
    let db = Arc::new(PersistenceDb::open_in_memory().unwrap());
    let s = SqliteSettingsStore::new(db);
    assert_eq!(s.get_rotation_offset_deg(&id("A")), 0.0);
}

#[test]
fn set_then_get_roundtrip() {
    let db = Arc::new(PersistenceDb::open_in_memory().unwrap());
    let s = SqliteSettingsStore::new(db);
    s.set_rotation_offset_deg(&id("A"), 45.5);
    assert_eq!(s.get_rotation_offset_deg(&id("A")), 45.5);
}

#[test]
fn extension_api_setting_roundtrip() {
    let db = PersistenceDb::open_in_memory().unwrap();
    db.set_setting("slime_server_addr", "10.0.0.1:6969")
        .unwrap();
    assert_eq!(
        db.get_setting("slime_server_addr").unwrap(),
        Some("10.0.0.1:6969".into())
    );
    assert_eq!(db.get_setting("missing").unwrap(), None);
}
