use device_traits::{BiasStore, DeviceId, InMemoryBiasStore, InMemorySettingsStore, SettingsStore};

fn id(serial: &str) -> DeviceId {
    DeviceId {
        mac: [0; 6],
        serial: serial.into(),
    }
}

#[test]
fn settings_default_zero() {
    let s = InMemorySettingsStore::default();
    assert_eq!(s.get_rotation_offset_deg(&id("A")), 0.0);
}

#[test]
fn settings_set_get_roundtrip() {
    let s = InMemorySettingsStore::default();
    s.set_rotation_offset_deg(&id("B"), 45.0);
    assert_eq!(s.get_rotation_offset_deg(&id("B")), 45.0);
}

#[test]
fn bias_default_none() {
    let b = InMemoryBiasStore::default();
    assert!(b.load_bias(&id("A")).is_none());
}

#[test]
fn bias_store_load_roundtrip() {
    let b = InMemoryBiasStore::default();
    b.store_bias(&id("X"), [0.001, -0.002, 0.0005]);
    assert_eq!(b.load_bias(&id("X")), Some([0.001, -0.002, 0.0005]));
}
