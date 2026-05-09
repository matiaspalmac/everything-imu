use device_traits::{BiasStore, DeviceId};
use persistence::{PersistenceDb, SqliteBiasStore};
use std::sync::Arc;

fn id() -> DeviceId {
    DeviceId {
        mac: [0x01, 0x02, 0x03, 0x04, 0x05, 0x06],
        serial: "TEST".into(),
    }
}

#[test]
fn unset_returns_none() {
    let db = Arc::new(PersistenceDb::open_in_memory().unwrap());
    let s = SqliteBiasStore::new(db);
    assert!(s.load_bias(&id()).is_none());
}

#[test]
fn store_then_load_roundtrip() {
    let db = Arc::new(PersistenceDb::open_in_memory().unwrap());
    let s = SqliteBiasStore::new(db);
    let bias = [0.001, -0.002, 0.0005];
    s.store_bias(&id(), bias);
    let got = s.load_bias(&id()).unwrap();
    assert!((got[0] - bias[0]).abs() < 1e-12);
    assert!((got[1] - bias[1]).abs() < 1e-12);
    assert!((got[2] - bias[2]).abs() < 1e-12);
}
