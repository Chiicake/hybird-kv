use std::cmp::Reverse;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessOp {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerConfig {
    pub candidate_limit: usize,
    pub max_value_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEligibilityReason {
    ValueTooLarge,
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

    pub fn ordered_candidates(&self) -> Vec<HotCandidate> {
        let mut candidates = self.candidates.clone();
        candidates.sort_by(|left, right| {
            Reverse(left.estimated_total_accesses)
                .cmp(&Reverse(right.estimated_total_accesses))
                .then_with(|| {
                    Reverse(left.estimated_read_accesses)
                        .cmp(&Reverse(right.estimated_read_accesses))
                })
                .then_with(|| left.key.cmp(&right.key))
        });
        candidates
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
    latest_snapshot: CandidateSnapshot,
}

impl HotTracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            latest_snapshot: CandidateSnapshot::default(),
        }
    }

    pub fn config(&self) -> &TrackerConfig {
        &self.config
    }

    pub fn latest_snapshot(&self) -> CandidateSnapshot {
        self.latest_snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        CandidateEligibilityReason, CandidateSnapshot, HotCandidate, HotTracker, TrackerConfig,
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
    fn candidate_ordering_prefers_hotter_read_heavy_keys_then_key_bytes() {
        let snapshot = CandidateSnapshot {
            generated_at: UNIX_EPOCH + Duration::from_secs(5),
            observed_total_accesses: 42,
            candidates: vec![
                HotCandidate {
                    key: b"beta".to_vec(),
                    estimated_total_accesses: 8,
                    estimated_read_accesses: 8,
                    last_known_value_size: Some(32),
                    ineligible_reason: None,
                },
                HotCandidate {
                    key: b"alpha".to_vec(),
                    estimated_total_accesses: 9,
                    estimated_read_accesses: 6,
                    last_known_value_size: Some(32),
                    ineligible_reason: None,
                },
                HotCandidate {
                    key: b"aardvark".to_vec(),
                    estimated_total_accesses: 8,
                    estimated_read_accesses: 8,
                    last_known_value_size: Some(32),
                    ineligible_reason: None,
                },
            ],
        };

        let ordered: Vec<Vec<u8>> = snapshot
            .ordered_candidates()
            .into_iter()
            .map(|candidate| candidate.key)
            .collect();

        assert_eq!(
            ordered,
            vec![b"alpha".to_vec(), b"aardvark".to_vec(), b"beta".to_vec()]
        );
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
    fn snapshot_contract_stays_serializable_with_expected_fields() {
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
        };
        let tracker = HotTracker::new(config.clone());

        assert_eq!(tracker.config(), &config);
        assert_eq!(tracker.latest_snapshot(), CandidateSnapshot::default());
    }
}
