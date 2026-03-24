//! Provisional observation-facing contracts for Phase 2A experiments.
//!
//! This module intentionally stays small and request-path agnostic. Phase 2B may
//! reshape the recorder abstraction, but these event fields are chosen to align
//! with the expected access-event shape and minimize churn later.

use std::sync::Mutex;
use std::time::SystemTime;

pub(crate) mod exact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Get,
    Set,
    Delete,
    Expire,
    Ttl,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessClass {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationEvent {
    pub command: CommandKind,
    pub key: Vec<u8>,
    pub access: AccessClass,
    pub value_size: Option<usize>,
    pub timestamp: SystemTime,
}

pub trait ExperimentObservationSink {
    fn record_observation(&self, event: ObservationEvent);
}

#[derive(Debug, Default)]
pub struct SharedObservationLog {
    events: Mutex<Vec<ObservationEvent>>,
}

impl SharedObservationLog {
    pub fn observations(&self) -> Vec<ObservationEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl ExperimentObservationSink for SharedObservationLog {
    fn record_observation(&self, event: ObservationEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl ObservationEvent {
    pub fn read(command: CommandKind, key: Vec<u8>, timestamp: SystemTime) -> Self {
        Self {
            command,
            key,
            access: AccessClass::Read,
            value_size: None,
            timestamp,
        }
    }

    pub fn write(
        command: CommandKind,
        key: Vec<u8>,
        value_size: Option<usize>,
        timestamp: SystemTime,
    ) -> Self {
        Self {
            command,
            key,
            access: AccessClass::Write,
            value_size,
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        exact::{ExactCounterSnapshot, ExactObservationCounter, ExactObservationLog},
        AccessClass, CommandKind, ExperimentObservationSink, ObservationEvent,
    };

    #[test]
    fn read_event_carries_future_access_event_fields() {
        let timestamp = UNIX_EPOCH + Duration::from_secs(17);

        let event = ObservationEvent::read(CommandKind::Get, b"alpha".to_vec(), timestamp);

        assert_eq!(event.command, CommandKind::Get);
        assert_eq!(event.key, b"alpha".to_vec());
        assert_eq!(event.access, AccessClass::Read);
        assert_eq!(event.value_size, None);
        assert_eq!(event.timestamp, timestamp);
    }

    #[test]
    fn write_event_keeps_known_value_size() {
        let timestamp = UNIX_EPOCH + Duration::from_secs(23);

        let event =
            ObservationEvent::write(CommandKind::Set, b"beta".to_vec(), Some(11), timestamp);

        assert_eq!(event.command, CommandKind::Set);
        assert_eq!(event.key, b"beta".to_vec());
        assert_eq!(event.access, AccessClass::Write);
        assert_eq!(event.value_size, Some(11));
        assert_eq!(event.timestamp, timestamp);
    }

    #[test]
    fn exact_ground_truth_log_preserves_observed_events_in_order() {
        let first = ObservationEvent::read(
            CommandKind::Get,
            b"alpha".to_vec(),
            UNIX_EPOCH + Duration::from_secs(1),
        );
        let second = ObservationEvent::write(
            CommandKind::Set,
            b"alpha".to_vec(),
            Some(5),
            UNIX_EPOCH + Duration::from_secs(2),
        );

        let log = ExactObservationLog::default();
        log.record_observation(first.clone());
        log.record_observation(second.clone());

        assert_eq!(log.events(), vec![first, second]);
    }

    #[test]
    fn exact_counter_snapshot_defaults_to_zero_counts() {
        let snapshot = ExactCounterSnapshot::default();

        assert_eq!(snapshot.total_events, 0);
        assert_eq!(snapshot.reads, 0);
        assert_eq!(snapshot.writes, 0);
        assert_eq!(snapshot.known_value_bytes, 0);
    }

    #[test]
    fn provisional_observation_sink_can_feed_exact_counter() {
        let timestamp = UNIX_EPOCH + Duration::from_secs(31);
        let counter = ExactObservationCounter::default();
        let sink: &dyn ExperimentObservationSink = &counter;

        sink.record_observation(ObservationEvent::read(
            CommandKind::Get,
            b"alpha".to_vec(),
            timestamp,
        ));
        sink.record_observation(ObservationEvent::write(
            CommandKind::Set,
            b"alpha".to_vec(),
            Some(7),
            timestamp,
        ));

        let snapshot = counter.snapshot();
        assert_eq!(snapshot.total_events, 2);
        assert_eq!(snapshot.reads, 1);
        assert_eq!(snapshot.writes, 1);
        assert_eq!(snapshot.known_value_bytes, 7);
    }
}
