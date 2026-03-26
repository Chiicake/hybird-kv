use std::collections::{HashMap, VecDeque};
use std::time::SystemTime;

pub(crate) const DEFAULT_MAX_TRACKED_KEY_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryEntry {
    pub(crate) key: Vec<u8>,
    pub(crate) read_count: u64,
    pub(crate) write_count: u64,
    pub(crate) last_seen: SystemTime,
    pub(crate) last_known_value_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct BoundedKeyRegistry {
    capacity: usize,
    max_key_bytes: usize,
    order: VecDeque<Vec<u8>>,
    entries: HashMap<Vec<u8>, RegistryEntry>,
}

impl BoundedKeyRegistry {
    pub(crate) fn new(capacity: usize, max_key_bytes: usize) -> Self {
        assert!(capacity > 0, "registry capacity must be positive");
        assert!(max_key_bytes > 0, "registry key bound must be positive");
        Self {
            capacity,
            max_key_bytes,
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn record_read(&mut self, key: &[u8], seen_at: SystemTime) {
        self.record(key, seen_at, None, true);
    }

    pub(crate) fn record_write(
        &mut self,
        key: &[u8],
        seen_at: SystemTime,
        value_size: Option<usize>,
    ) {
        self.record(key, seen_at, value_size, false);
    }

    pub(crate) fn entry(&self, key: &[u8]) -> Option<&RegistryEntry> {
        let key_vec = clamp_key(key, self.max_key_bytes);
        self.entries.get(key_vec.as_slice())
    }

    fn record(&mut self, key: &[u8], seen_at: SystemTime, value_size: Option<usize>, read: bool) {
        let key_vec = clamp_key(key, self.max_key_bytes);

        if let Some(entry) = self.entries.get_mut(key_vec.as_slice()) {
            entry.last_seen = seen_at;
            if let Some(size) = value_size {
                entry.last_known_value_size = Some(size);
            }
            if read {
                entry.read_count = entry.read_count.saturating_add(1);
            } else {
                entry.write_count = entry.write_count.saturating_add(1);
            }
            refresh_order(&mut self.order, key_vec.as_slice());
            return;
        }

        if self.entries.len() >= self.capacity {
            while let Some(oldest) = self.order.pop_front() {
                if self.entries.remove(&oldest).is_some() {
                    break;
                }
            }
        }

        let mut entry = RegistryEntry {
            key: key_vec.clone(),
            read_count: 0,
            write_count: 0,
            last_seen: seen_at,
            last_known_value_size: value_size,
        };
        if read {
            entry.read_count = 1;
        } else {
            entry.write_count = 1;
        }

        self.order.push_back(key_vec.clone());
        self.entries.insert(key_vec, entry);
    }
}

fn clamp_key(key: &[u8], max_key_bytes: usize) -> Vec<u8> {
    key[..key.len().min(max_key_bytes)].to_vec()
}

fn refresh_order(order: &mut VecDeque<Vec<u8>>, key: &[u8]) {
    if let Some(position) = order
        .iter()
        .position(|candidate| candidate.as_slice() == key)
    {
        let existing = order
            .remove(position)
            .expect("registry order index should exist");
        order.push_back(existing);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{BoundedKeyRegistry, DEFAULT_MAX_TRACKED_KEY_BYTES};

    #[test]
    fn registry_admits_recent_keys_up_to_capacity() {
        let mut registry = BoundedKeyRegistry::new(2, DEFAULT_MAX_TRACKED_KEY_BYTES);

        registry.record_read(b"alpha", UNIX_EPOCH + Duration::from_secs(1));
        registry.record_read(b"beta", UNIX_EPOCH + Duration::from_secs(2));

        assert_eq!(registry.len(), 2);
        assert!(registry.entry(b"alpha").is_some());
        assert!(registry.entry(b"beta").is_some());
    }

    #[test]
    fn registry_refresh_updates_recency_and_size_metadata() {
        let mut registry = BoundedKeyRegistry::new(2, DEFAULT_MAX_TRACKED_KEY_BYTES);

        registry.record_write(b"alpha", UNIX_EPOCH + Duration::from_secs(1), Some(8));
        registry.record_read(b"beta", UNIX_EPOCH + Duration::from_secs(8));
        registry.record_read(b"alpha", UNIX_EPOCH + Duration::from_secs(9));
        registry.record_read(b"gamma", UNIX_EPOCH + Duration::from_secs(11));

        let entry = registry.entry(b"alpha").unwrap();
        assert_eq!(entry.read_count, 1);
        assert_eq!(entry.write_count, 1);
        assert_eq!(entry.last_seen, UNIX_EPOCH + Duration::from_secs(9));
        assert_eq!(entry.last_known_value_size, Some(8));

        assert!(registry.entry(b"alpha").is_some());
        assert!(registry.entry(b"beta").is_none());
        assert!(registry.entry(b"gamma").is_some());
    }

    #[test]
    fn registry_eviction_remains_bounded_and_deterministic() {
        let mut registry = BoundedKeyRegistry::new(2, DEFAULT_MAX_TRACKED_KEY_BYTES);

        registry.record_read(b"alpha", UNIX_EPOCH + Duration::from_secs(1));
        registry.record_read(b"beta", UNIX_EPOCH + Duration::from_secs(2));
        registry.record_read(b"gamma", UNIX_EPOCH + Duration::from_secs(3));

        assert_eq!(registry.len(), 2);
        assert!(registry.entry(b"alpha").is_none());
        assert!(registry.entry(b"beta").is_some());
        assert!(registry.entry(b"gamma").is_some());
    }

    #[test]
    fn registry_clamps_overlong_keys_to_bound_key_storage() {
        let mut registry = BoundedKeyRegistry::new(1, DEFAULT_MAX_TRACKED_KEY_BYTES);
        let key = vec![b'a'; 1024];

        registry.record_read(&key, UNIX_EPOCH + Duration::from_secs(1));

        let stored = registry
            .entry(&key[..DEFAULT_MAX_TRACKED_KEY_BYTES])
            .unwrap();
        assert_eq!(stored.key.len(), DEFAULT_MAX_TRACKED_KEY_BYTES);
        assert_eq!(registry.len(), 1);
    }
}
