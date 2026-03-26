use std::time::{Duration, SystemTime};

use super::cms::CountMinSketch;
use super::{AccessOp, CandidateSnapshot, TrackerConfig};

/// Two-bucket approximation of a recent-access window.
///
/// The current and previous buckets are summed to estimate recent activity while
/// keeping bounded memory and simple rollover semantics.
#[derive(Debug, Clone)]
pub(crate) struct RollingWindowState {
    window_duration: Duration,
    current_started_at: SystemTime,
    current_total: CountMinSketch,
    current_reads: CountMinSketch,
    previous_total: CountMinSketch,
    previous_reads: CountMinSketch,
    current_observed_total: u64,
    previous_observed_total: u64,
    latest_snapshot: CandidateSnapshot,
}

impl RollingWindowState {
    pub(crate) fn new(config: &TrackerConfig, now: SystemTime) -> Self {
        Self {
            window_duration: config.window_duration,
            current_started_at: now,
            current_total: CountMinSketch::new(config.cms_width, config.cms_depth),
            current_reads: CountMinSketch::new(config.cms_width, config.cms_depth),
            previous_total: CountMinSketch::new(config.cms_width, config.cms_depth),
            previous_reads: CountMinSketch::new(config.cms_width, config.cms_depth),
            current_observed_total: 0,
            previous_observed_total: 0,
            latest_snapshot: CandidateSnapshot::default(),
        }
    }

    pub(crate) fn record_access(&mut self, key: &[u8], op: AccessOp, seen_at: SystemTime) {
        self.rotate_to(seen_at);
        self.current_total.increment(key);
        self.current_observed_total = self.current_observed_total.saturating_add(1);
        if matches!(op, AccessOp::Read) {
            self.current_reads.increment(key);
        }
    }

    pub(crate) fn rotate_to(&mut self, now: SystemTime) {
        let elapsed = now
            .duration_since(self.current_started_at)
            .unwrap_or(Duration::ZERO);

        if elapsed < self.window_duration {
            return;
        }

        let windows_elapsed = (elapsed.as_nanos() / self.window_duration.as_nanos()) as u64;
        if windows_elapsed == 1 {
            self.previous_total = self.current_total.clone();
            self.previous_reads = self.current_reads.clone();
            self.previous_observed_total = self.current_observed_total;
        } else {
            self.previous_total.reset();
            self.previous_reads.reset();
            self.previous_observed_total = 0;
        }

        self.current_total.reset();
        self.current_reads.reset();
        self.current_observed_total = 0;
        let advance_by = self
            .window_duration
            .saturating_mul(windows_elapsed.min(u32::MAX as u64) as u32);
        self.current_started_at = self.current_started_at + advance_by;
        if self.current_started_at > now {
            self.current_started_at = now;
        }
    }

    pub(crate) fn estimate_recent_total(&self, key: &[u8]) -> u64 {
        self.current_total
            .estimate(key)
            .saturating_add(self.previous_total.estimate(key))
    }

    pub(crate) fn estimate_recent_reads(&self, key: &[u8]) -> u64 {
        self.current_reads
            .estimate(key)
            .saturating_add(self.previous_reads.estimate(key))
    }

    pub(crate) fn observed_total_accesses(&self) -> u64 {
        self.current_observed_total
            .saturating_add(self.previous_observed_total)
    }

    pub(crate) fn store_snapshot(&mut self, snapshot: CandidateSnapshot) {
        self.latest_snapshot = snapshot;
    }

    pub(crate) fn latest_snapshot(&self) -> CandidateSnapshot {
        self.latest_snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use crate::tracker::{AccessOp, CandidateSnapshot, TrackerConfig};

    use super::RollingWindowState;

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
    fn current_window_snapshot_counts_contribute_to_recent_estimates() {
        let config = tracker_config();
        let mut windows = RollingWindowState::new(&config, UNIX_EPOCH);

        windows.record_access(
            b"alpha",
            AccessOp::Read,
            UNIX_EPOCH + Duration::from_secs(1),
        );
        windows.record_access(
            b"alpha",
            AccessOp::Read,
            UNIX_EPOCH + Duration::from_secs(2),
        );
        windows.record_access(
            b"alpha",
            AccessOp::Write,
            UNIX_EPOCH + Duration::from_secs(3),
        );

        assert_eq!(windows.estimate_recent_total(b"alpha"), 3);
        assert_eq!(windows.estimate_recent_reads(b"alpha"), 2);
    }

    #[test]
    fn old_activity_decays_after_two_rotations() {
        let config = tracker_config();
        let mut windows = RollingWindowState::new(&config, UNIX_EPOCH);

        windows.record_access(
            b"alpha",
            AccessOp::Read,
            UNIX_EPOCH + Duration::from_secs(1),
        );
        windows.rotate_to(UNIX_EPOCH + Duration::from_secs(11));
        assert_eq!(windows.estimate_recent_total(b"alpha"), 1);

        windows.rotate_to(UNIX_EPOCH + Duration::from_secs(21));
        assert_eq!(windows.estimate_recent_total(b"alpha"), 0);
        assert_eq!(windows.estimate_recent_reads(b"alpha"), 0);
    }

    #[test]
    fn exact_rollover_boundary_moves_current_into_previous_window() {
        let config = tracker_config();
        let mut windows = RollingWindowState::new(&config, UNIX_EPOCH);

        windows.record_access(
            b"alpha",
            AccessOp::Read,
            UNIX_EPOCH + Duration::from_secs(1),
        );
        assert_eq!(windows.estimate_recent_total(b"alpha"), 1);

        windows.record_access(
            b"alpha",
            AccessOp::Read,
            UNIX_EPOCH + config.window_duration,
        );

        assert_eq!(windows.estimate_recent_total(b"alpha"), 2);
        assert_eq!(windows.estimate_recent_reads(b"alpha"), 2);
    }

    #[test]
    fn latest_snapshot_storage_returns_last_published_snapshot() {
        let config = tracker_config();
        let mut windows = RollingWindowState::new(&config, UNIX_EPOCH);
        let snapshot = CandidateSnapshot {
            generated_at: UNIX_EPOCH + Duration::from_secs(12),
            observed_total_accesses: 7,
            candidates: Vec::new(),
        };

        windows.store_snapshot(snapshot.clone());

        assert_eq!(windows.latest_snapshot(), snapshot);
    }
}
