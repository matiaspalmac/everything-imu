//! Forwarded UDP haptic targets.
//!
//! A "UDP haptic target" is any host on the LAN that can receive a
//! tiny 8-byte intensity frame on a chosen port and translate it into
//! vibration — typically a Wemos / ESP32 firmware acting as a vest
//! node, but the wire format is deliberately trivial so anyone can
//! roll their own receiver.
//!
//! Wire frame (8 bytes, little-endian):
//!
//! | byte | meaning                                |
//! |------|----------------------------------------|
//! | 0    | magic `0xE1` (everything-imu haptic v1)|
//! | 1    | version `0x01`                         |
//! | 2..3 | intensity Q1.15 (0..=32767 maps 0..1)  |
//! | 4..5 | duration in ms, u16 little-endian      |
//! | 6..7 | reserved, set to 0                     |
//!
//! Targets are persisted in the settings DB as a single JSON blob
//! keyed by `udp_haptic_targets`. The list is small enough that a
//! linear scan on every send is cheaper than maintaining a hot map.

use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const FRAME_MAGIC: u8 = 0xE1;
const FRAME_VERSION: u8 = 0x01;
const SETTINGS_KEY: &str = "udp_haptic_targets";

/// One configured UDP haptic receiver. `mac` is a synthesized
/// locally-administered MAC used to identify the target when binding
/// OSC rules; it has nothing to do with the receiver's real network
/// MAC. The alias is purely cosmetic.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct UdpHapticTarget {
    pub mac: [u8; 6],
    pub alias: String,
    pub host: String,
    pub port: u16,
}

/// In-memory registry. Cheap to clone; the inner `Mutex` is held only
/// for the swap inside `set_all` / `push` / `remove`.
#[derive(Debug, Clone, Default)]
pub struct UdpHapticRegistry {
    inner: Arc<Mutex<Vec<UdpHapticTarget>>>,
}

impl UdpHapticRegistry {
    pub fn new(initial: Vec<UdpHapticTarget>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(initial)),
        }
    }
    pub fn list(&self) -> Vec<UdpHapticTarget> {
        self.inner.lock().clone()
    }
    pub fn set_all(&self, targets: Vec<UdpHapticTarget>) {
        *self.inner.lock() = targets;
    }
    pub fn upsert(&self, t: UdpHapticTarget) {
        let mut g = self.inner.lock();
        if let Some(slot) = g.iter_mut().find(|x| x.mac == t.mac) {
            *slot = t;
        } else {
            g.push(t);
        }
    }
    pub fn remove(&self, mac: [u8; 6]) -> bool {
        let mut g = self.inner.lock();
        let before = g.len();
        g.retain(|t| t.mac != mac);
        g.len() < before
    }
    pub fn find(&self, mac: [u8; 6]) -> Option<UdpHapticTarget> {
        self.inner.lock().iter().find(|t| t.mac == mac).cloned()
    }
}

/// Encode + send one haptic frame to the given target. Allocates a fresh
/// ephemeral UDP socket per call — the frames are infrequent so the
/// overhead is negligible and avoids the lifetime headache of caching a
/// socket inside the registry.
pub fn send(target: &UdpHapticTarget, intensity: f32, duration_ms: u16) -> std::io::Result<()> {
    let clamped = intensity.clamp(0.0, 1.0);
    let q = (clamped * 32767.0).round() as u16;
    let mut frame = [0u8; 8];
    frame[0] = FRAME_MAGIC;
    frame[1] = FRAME_VERSION;
    frame[2..4].copy_from_slice(&q.to_le_bytes());
    frame[4..6].copy_from_slice(&duration_ms.to_le_bytes());

    let addr: SocketAddr = format!("{}:{}", target.host, target.port)
        .parse()
        .or_else(|_| {
            // Fall back to a DNS lookup if the host is not a literal IP.
            std::net::ToSocketAddrs::to_socket_addrs(&(target.host.as_str(), target.port))?
                .next()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no resolved addr")
                })
        })?;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.send_to(&frame, addr)?;
    Ok(())
}

/// Generate a deterministic locally-administered MAC for a UDP target,
/// derived from `host:port`. Same host:port = same mac across restarts,
/// so the OSC rules that reference the mac keep working.
pub fn synth_mac(host: &str, port: u16) -> [u8; 6] {
    let seed = format!("udp:{host}:{port}");
    let h = fnv1a_64(seed.as_bytes()).to_le_bytes();
    [0x02, h[0], h[1], h[2], h[3], h[4]]
}

pub fn load_from_settings_json(json: &str) -> Vec<UdpHapticTarget> {
    serde_json::from_str(json).unwrap_or_default()
}

pub fn save_settings_key() -> &'static str {
    SETTINGS_KEY
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_mac_is_stable() {
        assert_eq!(
            synth_mac("192.168.0.42", 7000),
            synth_mac("192.168.0.42", 7000)
        );
    }

    #[test]
    fn synth_mac_differs_with_port() {
        assert_ne!(synth_mac("h", 1), synth_mac("h", 2));
    }
}
