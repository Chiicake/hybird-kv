use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::{AccessClass, ExperimentObservationSink, ObservationEvent};

/// Exact count snapshot for experiment-only ground truth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ExactCounterSnapshot {
    pub total_events: u64,
    pub reads: u64,
    pub writes: u64,
    pub known_value_bytes: u64,
}

/// Exact experiment-only counter for observed events.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct ExactObservationCounter {
    snapshot: Cell<ExactCounterSnapshot>,
}

impl ExactObservationCounter {
    #[allow(dead_code)]
    pub fn snapshot(&self) -> ExactCounterSnapshot {
        self.snapshot.get()
    }
}

impl ExperimentObservationSink for ExactObservationCounter {
    fn record_observation(&self, event: ObservationEvent) {
        let mut snapshot = self.snapshot.get();
        snapshot.total_events += 1;
        match event.access {
            AccessClass::Read => snapshot.reads += 1,
            AccessClass::Write => snapshot.writes += 1,
        }

        if let Some(value_size) = event.value_size {
            snapshot.known_value_bytes += value_size as u64;
        }

        self.snapshot.set(snapshot);
    }
}

/// Exact event capture for experiments and tests only.
///
/// This is intentionally a simple in-memory log, not the Phase 2B recorder
/// abstraction.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct ExactObservationLog {
    events: RefCell<Vec<ObservationEvent>>,
}

impl ExactObservationLog {
    #[allow(dead_code)]
    pub fn events(&self) -> Vec<ObservationEvent> {
        self.events.borrow().clone()
    }
}

impl ExperimentObservationSink for ExactObservationLog {
    fn record_observation(&self, event: ObservationEvent) {
        self.events.borrow_mut().push(event);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactHotKey {
    pub key: Vec<u8>,
    pub total_accesses: u64,
    pub read_accesses: u64,
    pub write_accesses: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ExactHotKeyCounts {
    total_accesses: u64,
    read_accesses: u64,
    write_accesses: u64,
}

#[derive(Debug, Default)]
pub struct ExactHotnessEvaluator {
    per_key: HashMap<Vec<u8>, ExactHotKeyCounts>,
}

impl ExactHotnessEvaluator {
    pub fn from_events(events: Vec<ObservationEvent>) -> Self {
        let mut evaluator = Self::default();
        for event in events {
            evaluator.record(event);
        }

        evaluator
    }

    pub fn record(&mut self, event: ObservationEvent) {
        let entry = self.per_key.entry(event.key).or_default();

        entry.total_accesses += 1;
        match event.access {
            AccessClass::Read => entry.read_accesses += 1,
            AccessClass::Write => entry.write_accesses += 1,
        }
    }

    pub fn top_keys(&self, limit: usize) -> Vec<ExactHotKey> {
        let mut ranking: Vec<_> = self.per_key.iter().collect();
        ranking.sort_by(|(left_key, left_counts), (right_key, right_counts)| {
            right_counts
                .total_accesses
                .cmp(&left_counts.total_accesses)
                .then_with(|| right_counts.read_accesses.cmp(&left_counts.read_accesses))
                .then_with(|| right_counts.write_accesses.cmp(&left_counts.write_accesses))
                .then_with(|| left_key.cmp(right_key))
        });

        let mut ranking: Vec<_> = ranking
            .into_iter()
            .map(|(key, counts)| ExactHotKey {
                key: key.clone(),
                total_accesses: counts.total_accesses,
                read_accesses: counts.read_accesses,
                write_accesses: counts.write_accesses,
            })
            .collect();
        ranking.truncate(limit);
        ranking
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::super::{CommandKind, ObservationEvent};
    use super::{ExactHotKey, ExactHotnessEvaluator};

    #[test]
    fn ranks_stable_heavy_hitters_from_observed_events() {
        let ranking = ExactHotnessEvaluator::from_events(vec![
            observed_read(CommandKind::Get, b"alpha"),
            observed_write(CommandKind::Set, b"beta", Some(5)),
            observed_read(CommandKind::Get, b"alpha"),
            observed_write(CommandKind::Delete, b"beta", None),
            observed_read(CommandKind::Ttl, b"alpha"),
            observed_write(CommandKind::Expire, b"gamma", None),
        ])
        .top_keys(2);

        assert_eq!(
            ranking,
            vec![
                ExactHotKey {
                    key: b"alpha".to_vec(),
                    total_accesses: 3,
                    read_accesses: 3,
                    write_accesses: 0,
                },
                ExactHotKey {
                    key: b"beta".to_vec(),
                    total_accesses: 2,
                    read_accesses: 0,
                    write_accesses: 2,
                },
            ]
        );
    }

    #[test]
    fn repeated_hot_key_pattern_counts_every_access() {
        let ranking = ExactHotnessEvaluator::from_events(vec![
            observed_read(CommandKind::Get, b"alpha"),
            observed_read(CommandKind::Get, b"alpha"),
            observed_read(CommandKind::Get, b"alpha"),
            observed_read(CommandKind::Get, b"beta"),
        ])
        .top_keys(2);

        assert_eq!(
            ranking,
            vec![
                ExactHotKey {
                    key: b"alpha".to_vec(),
                    total_accesses: 3,
                    read_accesses: 3,
                    write_accesses: 0,
                },
                ExactHotKey {
                    key: b"beta".to_vec(),
                    total_accesses: 1,
                    read_accesses: 1,
                    write_accesses: 0,
                },
            ]
        );
    }

    #[test]
    fn mixed_read_write_access_keeps_split_counts() {
        let ranking = ExactHotnessEvaluator::from_events(vec![
            observed_write(CommandKind::Set, b"alpha", Some(5)),
            observed_read(CommandKind::Get, b"alpha"),
            observed_write(CommandKind::Expire, b"alpha", None),
            observed_write(CommandKind::Delete, b"beta", None),
            observed_read(CommandKind::Ttl, b"alpha"),
        ])
        .top_keys(2);

        assert_eq!(
            ranking,
            vec![
                ExactHotKey {
                    key: b"alpha".to_vec(),
                    total_accesses: 4,
                    read_accesses: 2,
                    write_accesses: 2,
                },
                ExactHotKey {
                    key: b"beta".to_vec(),
                    total_accesses: 1,
                    read_accesses: 0,
                    write_accesses: 1,
                },
            ]
        );
    }

    #[test]
    fn multiple_keys_preserves_stable_heavy_hitters() {
        let ranking = ExactHotnessEvaluator::from_events(vec![
            observed_read(CommandKind::Get, b"hot-a"),
            observed_read(CommandKind::Get, b"hot-b"),
            observed_write(CommandKind::Set, b"hot-a", Some(3)),
            observed_read(CommandKind::Get, b"cold"),
            observed_read(CommandKind::Ttl, b"hot-b"),
            observed_write(CommandKind::Expire, b"hot-a", None),
            observed_write(CommandKind::Delete, b"hot-b", None),
            observed_read(CommandKind::Get, b"hot-a"),
        ])
        .top_keys(3);

        assert_eq!(
            ranking,
            vec![
                ExactHotKey {
                    key: b"hot-a".to_vec(),
                    total_accesses: 4,
                    read_accesses: 2,
                    write_accesses: 2,
                },
                ExactHotKey {
                    key: b"hot-b".to_vec(),
                    total_accesses: 3,
                    read_accesses: 2,
                    write_accesses: 1,
                },
                ExactHotKey {
                    key: b"cold".to_vec(),
                    total_accesses: 1,
                    read_accesses: 1,
                    write_accesses: 0,
                },
            ]
        );
    }

    fn observed_read(command: CommandKind, key: &[u8]) -> ObservationEvent {
        ObservationEvent::read(command, key.to_vec(), UNIX_EPOCH)
    }

    fn observed_write(
        command: CommandKind,
        key: &[u8],
        value_size: Option<usize>,
    ) -> ObservationEvent {
        ObservationEvent::write(command, key.to_vec(), value_size, UNIX_EPOCH)
    }
}
