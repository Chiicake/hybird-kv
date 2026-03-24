use super::{AccessClass, ExperimentObservationSink, ObservationEvent};

/// Exact count snapshot for experiment-only ground truth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExactCounterSnapshot {
    pub total_events: u64,
    pub reads: u64,
    pub writes: u64,
    pub known_value_bytes: u64,
}

/// Exact experiment-only counter for observed events.
#[derive(Debug, Clone, Default)]
pub struct ExactObservationCounter {
    snapshot: ExactCounterSnapshot,
}

impl ExactObservationCounter {
    pub fn snapshot(&self) -> ExactCounterSnapshot {
        self.snapshot
    }
}

impl ExperimentObservationSink for ExactObservationCounter {
    fn record_observation(&mut self, event: ObservationEvent) {
        self.snapshot.total_events += 1;
        match event.access {
            AccessClass::Read => self.snapshot.reads += 1,
            AccessClass::Write => self.snapshot.writes += 1,
        }

        if let Some(value_size) = event.value_size {
            self.snapshot.known_value_bytes += value_size as u64;
        }
    }
}

/// Exact event capture for experiments and tests only.
///
/// This is intentionally a simple in-memory log, not the Phase 2B recorder
/// abstraction.
#[derive(Debug, Clone, Default)]
pub struct ExactObservationLog {
    events: Vec<ObservationEvent>,
}

impl ExactObservationLog {
    pub fn record(&mut self, event: ObservationEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[ObservationEvent] {
        &self.events
    }
}

impl ExperimentObservationSink for ExactObservationLog {
    fn record_observation(&mut self, event: ObservationEvent) {
        self.record(event);
    }
}
