//! Live OSC parameter sniffer.
//!
//! Tracks per-address `(count, min, max, latest, last_seen)` so the UI can
//! show users exactly which addresses VRChat is sending and what value range
//! each one carries. Removes the "is my rule's address even right?" guess
//! work — if nothing shows up, VRChat isn't reaching the listener; if a name
//! shows up under `/avatar/parameters/` the user can copy it verbatim into a
//! rule.
//!
//! Pure data structure — the bridge feeds it from the same hot path that
//! routes packets to rumble sinks (see `listener::handle_packet`).

use std::collections::HashMap;
use std::time::Instant;

/// One row of sniffer data, one per distinct OSC address ever seen.
#[derive(Debug, Clone, PartialEq)]
pub struct SnifferEntry {
    pub address: String,
    /// How many packets we've routed for this address since process start.
    pub count: u64,
    pub min_value: f32,
    pub max_value: f32,
    pub latest_value: f32,
    /// `Instant` is `Copy + !Serialize`. UIs render as "ms since last seen"
    /// computed against `Instant::now()`.
    pub last_seen: Instant,
}

/// Rolling per-address tracker.
///
/// `cap` bounds how many distinct addresses we'll remember; once full, the
/// oldest entry (smallest `last_seen`) is evicted before inserting a new one.
/// VRChat avatars rarely emit more than a few hundred parameters, so a 512
/// default cap is overkill in practice.
#[derive(Debug)]
pub struct Sniffer {
    entries: HashMap<String, SnifferEntry>,
    cap: usize,
}

impl Sniffer {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(cap.min(1024)),
            cap: cap.max(1),
        }
    }

    /// Ingest one packet's address + value. Updates the existing entry or
    /// inserts a new one (evicting the oldest if at capacity).
    pub fn ingest(&mut self, address: &str, value: f32) {
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(address) {
            entry.count = entry.count.saturating_add(1);
            entry.latest_value = value;
            if value < entry.min_value {
                entry.min_value = value;
            }
            if value > entry.max_value {
                entry.max_value = value;
            }
            entry.last_seen = now;
            return;
        }
        if self.entries.len() >= self.cap {
            self.evict_oldest();
        }
        self.entries.insert(
            address.to_string(),
            SnifferEntry {
                address: address.to_string(),
                count: 1,
                min_value: value,
                max_value: value,
                latest_value: value,
                last_seen: now,
            },
        );
    }

    fn evict_oldest(&mut self) {
        if let Some((oldest_key, _)) = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.last_seen)
            .map(|(k, e)| (k.clone(), e.last_seen))
        {
            self.entries.remove(&oldest_key);
        }
    }

    /// Snapshot of the current entries sorted by `last_seen` descending
    /// (most recently active first). Cheap — clones the map; sniffer is
    /// expected to be small (≤ a few hundred entries).
    pub fn snapshot(&self) -> Vec<SnifferEntry> {
        let mut out: Vec<SnifferEntry> = self.entries.values().cloned().collect();
        out.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        out
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Sniffer {
    fn default() -> Self {
        Self::new(512)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn first_ingest_creates_entry_with_value_as_min_max() {
        let mut s = Sniffer::new(8);
        s.ingest("/a", 0.5);
        let snap = s.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].address, "/a");
        assert_eq!(snap[0].count, 1);
        assert_eq!(snap[0].min_value, 0.5);
        assert_eq!(snap[0].max_value, 0.5);
        assert_eq!(snap[0].latest_value, 0.5);
    }

    #[test]
    fn repeated_ingest_tracks_range_and_latest() {
        let mut s = Sniffer::new(8);
        s.ingest("/x", 0.5);
        s.ingest("/x", 0.1);
        s.ingest("/x", 0.9);
        s.ingest("/x", 0.4);
        let snap = s.snapshot();
        let e = &snap[0];
        assert_eq!(e.count, 4);
        assert_eq!(e.min_value, 0.1);
        assert_eq!(e.max_value, 0.9);
        assert_eq!(e.latest_value, 0.4, "latest is the last value, not max");
    }

    #[test]
    fn snapshot_sorts_by_last_seen_descending() {
        let mut s = Sniffer::new(8);
        s.ingest("/old", 0.0);
        sleep(Duration::from_millis(2));
        s.ingest("/new", 0.0);
        let snap = s.snapshot();
        assert_eq!(snap[0].address, "/new");
        assert_eq!(snap[1].address, "/old");
    }

    #[test]
    fn ingest_at_capacity_evicts_oldest() {
        let mut s = Sniffer::new(2);
        s.ingest("/a", 0.0);
        sleep(Duration::from_millis(2));
        s.ingest("/b", 0.0);
        sleep(Duration::from_millis(2));
        s.ingest("/c", 0.0);
        let snap = s.snapshot();
        let addrs: Vec<_> = snap.iter().map(|e| e.address.as_str()).collect();
        assert_eq!(addrs.len(), 2);
        assert!(addrs.contains(&"/b"));
        assert!(addrs.contains(&"/c"));
        assert!(!addrs.contains(&"/a"), "oldest address must be evicted");
    }

    #[test]
    fn clear_drops_all_entries() {
        let mut s = Sniffer::new(8);
        s.ingest("/a", 1.0);
        s.ingest("/b", 1.0);
        s.clear();
        assert!(s.is_empty());
        assert!(s.snapshot().is_empty());
    }

    #[test]
    fn cap_zero_treated_as_one() {
        let mut s = Sniffer::new(0);
        s.ingest("/a", 0.0);
        sleep(Duration::from_millis(2));
        s.ingest("/b", 0.0);
        // Cap clamped to 1 internally; second insert evicts first.
        let snap = s.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].address, "/b");
    }
}
