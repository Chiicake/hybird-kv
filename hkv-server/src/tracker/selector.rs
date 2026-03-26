use std::cmp::Ordering;
use std::time::{Duration, SystemTime};

use super::{CandidateEligibilityReason, HotCandidate, TrackerConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateInput {
    pub(crate) key: Vec<u8>,
    pub(crate) recent_total_accesses: u64,
    pub(crate) recent_read_accesses: u64,
    pub(crate) last_seen: SystemTime,
    pub(crate) last_known_value_size: Option<usize>,
}

pub(crate) fn select_candidates(
    config: &TrackerConfig,
    inputs: Vec<CandidateInput>,
    now: SystemTime,
) -> Vec<HotCandidate> {
    let mut ranked: Vec<_> = inputs
        .into_iter()
        .map(|input| rank_candidate(config, input, now))
        .collect();

    ranked.sort_by(|left, right| compare_ranked(left, right));
    ranked
        .into_iter()
        .take(config.candidate_limit)
        .map(|ranked| ranked.candidate)
        .collect()
}

#[derive(Debug, Clone)]
struct RankedCandidate {
    candidate: HotCandidate,
    eligible: bool,
    recent_total_accesses: u64,
    idle_age: Duration,
}

fn rank_candidate(
    config: &TrackerConfig,
    input: CandidateInput,
    now: SystemTime,
) -> RankedCandidate {
    let idle_age = now
        .duration_since(input.last_seen)
        .unwrap_or(Duration::ZERO);
    let read_ratio_percent = if input.recent_total_accesses == 0 {
        0
    } else {
        ((input.recent_read_accesses.saturating_mul(100)) / input.recent_total_accesses) as u8
    };
    let ineligible_reason = if input.recent_total_accesses < config.min_recent_accesses {
        Some(CandidateEligibilityReason::TooFewRecentAccesses)
    } else if idle_age > config.max_idle_age {
        Some(CandidateEligibilityReason::Stale)
    } else if read_ratio_percent < config.min_read_ratio_percent {
        Some(CandidateEligibilityReason::ReadRatioTooLow)
    } else if input.last_known_value_size.is_none() {
        Some(CandidateEligibilityReason::ValueSizeUnknown)
    } else if input.last_known_value_size > Some(config.max_value_size) {
        Some(CandidateEligibilityReason::ValueTooLarge)
    } else {
        None
    };

    RankedCandidate {
        eligible: ineligible_reason.is_none(),
        recent_total_accesses: input.recent_total_accesses,
        idle_age,
        candidate: HotCandidate {
            key: input.key,
            estimated_total_accesses: input.recent_total_accesses,
            estimated_read_accesses: input.recent_read_accesses,
            last_known_value_size: input.last_known_value_size,
            ineligible_reason,
        },
    }
}

fn compare_ranked(left: &RankedCandidate, right: &RankedCandidate) -> Ordering {
    right
        .eligible
        .cmp(&left.eligible)
        .then_with(|| right.recent_total_accesses.cmp(&left.recent_total_accesses))
        .then_with(|| left.idle_age.cmp(&right.idle_age))
        .then_with(|| left.candidate.key.cmp(&right.candidate.key))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use crate::tracker::{CandidateEligibilityReason, TrackerConfig};

    use super::{select_candidates, CandidateInput};

    fn tracker_config() -> TrackerConfig {
        TrackerConfig {
            candidate_limit: 4,
            max_value_size: 256,
            registry_capacity: 16,
            max_key_bytes: 64,
            cms_width: 64,
            cms_depth: 4,
            window_duration: Duration::from_secs(10),
            min_recent_accesses: 2,
            min_read_ratio_percent: 50,
            max_idle_age: Duration::from_secs(20),
        }
    }

    #[test]
    fn read_ratio_rule_keeps_read_heavy_candidate_eligible() {
        let config = tracker_config();
        let now = UNIX_EPOCH + Duration::from_secs(40);
        let candidates = select_candidates(
            &config,
            vec![
                CandidateInput {
                    key: b"read-heavy".to_vec(),
                    recent_total_accesses: 10,
                    recent_read_accesses: 9,
                    last_seen: now - Duration::from_secs(1),
                    last_known_value_size: Some(64),
                },
                CandidateInput {
                    key: b"write-heavy".to_vec(),
                    recent_total_accesses: 10,
                    recent_read_accesses: 2,
                    last_seen: now - Duration::from_secs(1),
                    last_known_value_size: Some(64),
                },
            ],
            now,
        );

        let read_heavy = candidates
            .iter()
            .find(|candidate| candidate.key == b"read-heavy".to_vec())
            .unwrap();
        let write_heavy = candidates
            .iter()
            .find(|candidate| candidate.key == b"write-heavy".to_vec())
            .unwrap();

        assert!(read_heavy.is_eligible());
        assert_eq!(
            write_heavy.ineligible_reason,
            Some(CandidateEligibilityReason::ReadRatioTooLow)
        );
    }

    #[test]
    fn oversized_or_unknown_value_size_keys_are_marked_ineligible_with_explicit_reason() {
        let config = tracker_config();
        let now = UNIX_EPOCH + Duration::from_secs(40);
        let candidates = select_candidates(
            &config,
            vec![
                CandidateInput {
                    key: b"oversized".to_vec(),
                    recent_total_accesses: 10,
                    recent_read_accesses: 10,
                    last_seen: now,
                    last_known_value_size: Some(4096),
                },
                CandidateInput {
                    key: b"unknown-size".to_vec(),
                    recent_total_accesses: 10,
                    recent_read_accesses: 10,
                    last_seen: now,
                    last_known_value_size: None,
                },
            ],
            now,
        );

        let unknown = candidates
            .iter()
            .find(|candidate| candidate.key == b"unknown-size".to_vec())
            .unwrap();
        let oversized = candidates
            .iter()
            .find(|candidate| candidate.key == b"oversized".to_vec())
            .unwrap();

        assert_eq!(
            unknown.ineligible_reason,
            Some(CandidateEligibilityReason::ValueSizeUnknown)
        );
        assert_eq!(
            oversized.ineligible_reason,
            Some(CandidateEligibilityReason::ValueTooLarge)
        );
    }

    #[test]
    fn recency_rule_marks_stale_candidate_ineligible() {
        let config = tracker_config();
        let now = UNIX_EPOCH + Duration::from_secs(40);
        let candidates = select_candidates(
            &config,
            vec![
                CandidateInput {
                    key: b"fresh".to_vec(),
                    recent_total_accesses: 8,
                    recent_read_accesses: 8,
                    last_seen: now - Duration::from_secs(1),
                    last_known_value_size: Some(64),
                },
                CandidateInput {
                    key: b"stale".to_vec(),
                    recent_total_accesses: 8,
                    recent_read_accesses: 8,
                    last_seen: now - Duration::from_secs(30),
                    last_known_value_size: Some(64),
                },
            ],
            now,
        );

        let fresh = candidates
            .iter()
            .find(|candidate| candidate.key == b"fresh".to_vec())
            .unwrap();
        let stale = candidates
            .iter()
            .find(|candidate| candidate.key == b"stale".to_vec())
            .unwrap();

        assert!(fresh.is_eligible());
        assert_eq!(
            stale.ineligible_reason,
            Some(CandidateEligibilityReason::Stale)
        );
    }

    #[test]
    fn exact_min_recent_access_threshold_is_inclusive() {
        let config = tracker_config();
        let now = UNIX_EPOCH + Duration::from_secs(40);
        let candidates = select_candidates(
            &config,
            vec![CandidateInput {
                key: b"threshold".to_vec(),
                recent_total_accesses: config.min_recent_accesses,
                recent_read_accesses: config.min_recent_accesses,
                last_seen: now,
                last_known_value_size: Some(64),
            }],
            now,
        );

        assert!(candidates[0].is_eligible());
    }

    #[test]
    fn exact_read_ratio_threshold_is_inclusive() {
        let config = tracker_config();
        let now = UNIX_EPOCH + Duration::from_secs(40);
        let candidates = select_candidates(
            &config,
            vec![CandidateInput {
                key: b"ratio".to_vec(),
                recent_total_accesses: 10,
                recent_read_accesses: 5,
                last_seen: now,
                last_known_value_size: Some(64),
            }],
            now,
        );

        assert!(candidates[0].is_eligible());
    }

    #[test]
    fn exact_idle_age_threshold_is_inclusive() {
        let config = tracker_config();
        let now = UNIX_EPOCH + Duration::from_secs(40);
        let candidates = select_candidates(
            &config,
            vec![CandidateInput {
                key: b"idle".to_vec(),
                recent_total_accesses: 10,
                recent_read_accesses: 10,
                last_seen: now - config.max_idle_age,
                last_known_value_size: Some(64),
            }],
            now,
        );

        assert!(candidates[0].is_eligible());
    }
}
