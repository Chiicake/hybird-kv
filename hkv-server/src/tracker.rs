mod cms;
mod registry;
mod selector;
mod windows;

use std::time::SystemTime;

use self::cms::CountMinSketch;
use self::registry::BoundedKeyRegistry;
use self::selector::{select_candidates, CandidateInput};
use self::windows::RollingWindowState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessOp {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerConfig {
    pub candidate_limit: usize,
    pub max_value_size: usize,
    pub registry_capacity: usize,
    pub max_key_bytes: usize,
    pub cms_width: usize,
    pub cms_depth: usize,
    pub window_duration: std::time::Duration,
    pub min_recent_accesses: u64,
    pub min_read_ratio_percent: u8,
    pub max_idle_age: std::time::Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEligibilityReason {
    ValueSizeUnknown,
    ValueTooLarge,
    TooFewRecentAccesses,
    ReadRatioTooLow,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotCandidate {
    pub key: Vec<u8>,
    pub estimated_total_accesses: u64,
    pub estimated_read_accesses: u64,
    pub last_known_value_size: Option<usize>,
    pub ineligible_reason: Option<CandidateEligibilityReason>,
}

impl HotCandidate {
    pub fn is_eligible(&self) -> bool {
        self.ineligible_reason.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSnapshot {
    pub generated_at: SystemTime,
    pub observed_total_accesses: u64,
    pub candidates: Vec<HotCandidate>,
}

impl CandidateSnapshot {
    pub fn eligible_candidate_count(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.is_eligible())
            .count()
    }
}

impl Default for CandidateSnapshot {
    fn default() -> Self {
        Self {
            generated_at: SystemTime::UNIX_EPOCH,
            observed_total_accesses: 0,
            candidates: Vec::new(),
        }
    }
}

/// Minimal candidate-export coordinator shell.
///
/// This deliberately does not perform tracking work yet; later tasks will add
/// estimator and request-path integration behind this coordinator.
#[derive(Debug, Clone)]
pub struct HotTracker {
    config: TrackerConfig,
    windows: RollingWindowState,
    _estimator: CountMinSketch,
    _registry: BoundedKeyRegistry,
}

impl HotTracker {
    pub fn new(config: TrackerConfig) -> Self {
        assert!(
            config.window_duration > std::time::Duration::ZERO,
            "tracker window duration must be positive"
        );
        let estimator = CountMinSketch::new(config.cms_width, config.cms_depth);
        let registry = BoundedKeyRegistry::new(config.registry_capacity, config.max_key_bytes);
        let windows = RollingWindowState::new(&config, SystemTime::UNIX_EPOCH);
        Self {
            config,
            windows,
            _estimator: estimator,
            _registry: registry,
        }
    }

    pub fn config(&self) -> &TrackerConfig {
        &self.config
    }

    pub fn latest_snapshot(&self) -> CandidateSnapshot {
        self.windows.latest_snapshot()
    }

    pub(crate) fn record_access(
        &mut self,
        key: &[u8],
        op: AccessOp,
        seen_at: SystemTime,
        value_size: Option<usize>,
    ) {
        self.windows.record_access(key, op, seen_at);
        match op {
            AccessOp::Read => self._registry.record_read(key, seen_at),
            AccessOp::Write => self._registry.record_write(key, seen_at, value_size),
        }
    }

    pub(crate) fn publish_snapshot(&mut self, generated_at: SystemTime) {
        self.windows.rotate_to(generated_at);
        let inputs: Vec<CandidateInput> = self
            ._registry
            .entries()
            .map(|entry| CandidateInput {
                key: entry.key.clone(),
                recent_total_accesses: self.windows.estimate_recent_total(&entry.key),
                recent_read_accesses: self.windows.estimate_recent_reads(&entry.key),
                last_seen: entry.last_seen,
                last_known_value_size: entry.last_known_value_size,
            })
            .collect();

        let candidates = select_candidates(&self.config, inputs, generated_at);
        let observed_total_accesses = self.windows.observed_total_accesses();
        self.windows.store_snapshot(CandidateSnapshot {
            generated_at,
            observed_total_accesses,
            candidates,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        AccessOp, CandidateEligibilityReason, CandidateSnapshot, HotCandidate, HotTracker,
        TrackerConfig,
    };

    #[test]
    fn empty_snapshot_defaults_to_no_candidates() {
        let snapshot = CandidateSnapshot::default();

        assert_eq!(snapshot.generated_at, UNIX_EPOCH);
        assert_eq!(snapshot.observed_total_accesses, 0);
        assert_eq!(snapshot.eligible_candidate_count(), 0);
        assert!(snapshot.candidates.is_empty());
    }

    #[test]
    fn eligibility_reason_is_absent_for_eligible_candidates_and_present_otherwise() {
        let eligible = HotCandidate {
            key: b"alpha".to_vec(),
            estimated_total_accesses: 11,
            estimated_read_accesses: 10,
            last_known_value_size: Some(64),
            ineligible_reason: None,
        };
        let oversized = HotCandidate {
            key: b"beta".to_vec(),
            estimated_total_accesses: 11,
            estimated_read_accesses: 10,
            last_known_value_size: Some(4096),
            ineligible_reason: Some(CandidateEligibilityReason::ValueTooLarge),
        };

        assert!(eligible.is_eligible());
        assert_eq!(eligible.ineligible_reason, None);

        assert!(!oversized.is_eligible());
        assert_eq!(
            oversized.ineligible_reason,
            Some(CandidateEligibilityReason::ValueTooLarge)
        );
    }

    #[test]
    fn snapshot_contract_stays_constructible_with_expected_fields() {
        let snapshot = CandidateSnapshot {
            generated_at: UNIX_EPOCH + Duration::from_secs(9),
            observed_total_accesses: 64,
            candidates: vec![HotCandidate {
                key: b"alpha".to_vec(),
                estimated_total_accesses: 11,
                estimated_read_accesses: 9,
                last_known_value_size: Some(128),
                ineligible_reason: Some(CandidateEligibilityReason::ValueTooLarge),
            }],
        };

        let CandidateSnapshot {
            generated_at,
            observed_total_accesses,
            candidates,
        } = snapshot;

        assert_eq!(generated_at, UNIX_EPOCH + Duration::from_secs(9));
        assert_eq!(observed_total_accesses, 64);
        assert_eq!(candidates.len(), 1);

        let HotCandidate {
            key,
            estimated_total_accesses,
            estimated_read_accesses,
            last_known_value_size,
            ineligible_reason,
        } = &candidates[0];

        assert_eq!(key, &b"alpha".to_vec());
        assert_eq!(*estimated_total_accesses, 11);
        assert_eq!(*estimated_read_accesses, 9);
        assert_eq!(*last_known_value_size, Some(128));
        assert_eq!(
            *ineligible_reason,
            Some(CandidateEligibilityReason::ValueTooLarge)
        );
    }

    #[test]
    fn tracker_shell_exposes_config_and_default_snapshot() {
        let config = TrackerConfig {
            candidate_limit: 16,
            max_value_size: 1024,
            registry_capacity: 64,
            max_key_bytes: 256,
            cms_width: 128,
            cms_depth: 4,
            window_duration: Duration::from_secs(30),
            min_recent_accesses: 2,
            min_read_ratio_percent: 50,
            max_idle_age: Duration::from_secs(60),
        };
        let tracker = HotTracker::new(config.clone());

        assert_eq!(tracker.config(), &config);
        assert_eq!(tracker.latest_snapshot(), CandidateSnapshot::default());
    }

    #[test]
    fn tracker_shell_can_record_access_and_publish_snapshot() {
        let config = TrackerConfig {
            candidate_limit: 16,
            max_value_size: 1024,
            registry_capacity: 64,
            max_key_bytes: 256,
            cms_width: 128,
            cms_depth: 4,
            window_duration: std::time::Duration::from_secs(10),
            min_recent_accesses: 1,
            min_read_ratio_percent: 0,
            max_idle_age: std::time::Duration::from_secs(60),
        };
        let mut tracker = HotTracker::new(config);
        let now = UNIX_EPOCH + Duration::from_secs(5);

        tracker.record_access(b"alpha", AccessOp::Read, now, None);
        tracker.record_access(b"alpha", AccessOp::Write, now, Some(64));
        tracker.publish_snapshot(now);

        let snapshot = tracker.latest_snapshot();
        assert_eq!(snapshot.observed_total_accesses, 2);
        assert_eq!(snapshot.candidates.len(), 1);
        assert_eq!(snapshot.candidates[0].key, b"alpha".to_vec());
    }

    #[test]
    #[should_panic(expected = "tracker window duration must be positive")]
    fn tracker_rejects_zero_window_duration() {
        let _ = HotTracker::new(TrackerConfig {
            candidate_limit: 16,
            max_value_size: 1024,
            registry_capacity: 64,
            max_key_bytes: 256,
            cms_width: 128,
            cms_depth: 4,
            window_duration: std::time::Duration::ZERO,
            min_recent_accesses: 1,
            min_read_ratio_percent: 0,
            max_idle_age: std::time::Duration::from_secs(60),
        });
    }
}
