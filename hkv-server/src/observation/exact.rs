use std::cell::{Cell, RefCell};

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
#[derive(Debug, Default)]
pub struct ExactObservationCounter {
    snapshot: Cell<ExactCounterSnapshot>,
}

impl ExactObservationCounter {
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
pub struct ExactObservationLog {
    events: RefCell<Vec<ObservationEvent>>,
}

impl ExactObservationLog {
    pub fn events(&self) -> Vec<ObservationEvent> {
        self.events.borrow().clone()
    }
}

impl ExperimentObservationSink for ExactObservationLog {
    fn record_observation(&self, event: ObservationEvent) {
        self.events.borrow_mut().push(event);
    }
}
