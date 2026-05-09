//! Settings + bias persistence trait + in-memory impls.

use crate::device::DeviceId;
use std::collections::HashMap;
use std::sync::RwLock;

pub trait SettingsStore: Send + Sync {
    fn get_rotation_offset_deg(&self, id: &DeviceId) -> f32;
    fn set_rotation_offset_deg(&self, id: &DeviceId, deg: f32);

    /// Free-form key/value lookup. Used by the pipeline to read per-device
    /// settings (`fusion_algo:<mac>`, `mounting_orientation:<mac>`,
    /// `magnetometer_enabled:<mac>`) without each individual concern needing
    /// its own typed accessor on this trait.
    ///
    /// Returns `None` when the key is absent. Default impl returns `None`
    /// so existing in-memory implementations stay valid.
    fn get(&self, _key: &str) -> Option<String> {
        None
    }

    /// Companion to [`SettingsStore::get`]. Default impl is a no-op.
    fn set(&self, _key: &str, _value: &str) {}
}

pub trait BiasStore: Send + Sync {
    /// Returns persisted gyro bias seed (rad/s) for this device, if any.
    fn load_bias(&self, id: &DeviceId) -> Option<[f64; 3]>;
    /// Persist the latest bias estimate. Called periodically by `core::Pipeline`.
    fn store_bias(&self, id: &DeviceId, bias: [f64; 3]);
}

#[derive(Default)]
pub struct InMemorySettingsStore {
    rot: RwLock<HashMap<DeviceId, f32>>,
    kv: RwLock<HashMap<String, String>>,
}

impl SettingsStore for InMemorySettingsStore {
    fn get_rotation_offset_deg(&self, id: &DeviceId) -> f32 {
        *self.rot.read().unwrap().get(id).unwrap_or(&0.0)
    }
    fn set_rotation_offset_deg(&self, id: &DeviceId, deg: f32) {
        self.rot.write().unwrap().insert(id.clone(), deg);
    }
    fn get(&self, key: &str) -> Option<String> {
        self.kv.read().unwrap().get(key).cloned()
    }
    fn set(&self, key: &str, value: &str) {
        self.kv
            .write()
            .unwrap()
            .insert(key.to_string(), value.to_string());
    }
}

#[derive(Default)]
pub struct InMemoryBiasStore {
    bias: RwLock<HashMap<DeviceId, [f64; 3]>>,
}

impl BiasStore for InMemoryBiasStore {
    fn load_bias(&self, id: &DeviceId) -> Option<[f64; 3]> {
        self.bias.read().unwrap().get(id).copied()
    }
    fn store_bias(&self, id: &DeviceId, bias: [f64; 3]) {
        self.bias.write().unwrap().insert(id.clone(), bias);
    }
}
