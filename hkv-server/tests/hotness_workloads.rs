use hkv_server::phase2a_testing::{
    CommandKind, ExactHotKey, ExactHotnessEvaluator, ObservationEvent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct StabilityCapture {
    capture_index: usize,
    window_events_before_reset: usize,
    reset_from_event_count: usize,
    window_events_after_reset: usize,
    cumulative_top_keys: Vec<ExactHotKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SustainedStabilityReport {
    total_events_seen: usize,
    current_window_events: usize,
    captures: Vec<StabilityCapture>,
}

#[derive(Debug, Default)]
struct WindowAccumulator {
    event_count: usize,
}

impl WindowAccumulator {
    fn record_event(&mut self) {
        self.event_count += 1;
    }

    fn event_count(&self) -> usize {
        self.event_count
    }

    fn reset(&mut self) -> usize {
        let previous = self.event_count;
        self.event_count = 0;
        previous
    }
}

#[test]
fn phase2a_matrix_zipf_like_skew_surfaces_clear_head_keys() {
    let ranking = evaluate_workload(zipf_like_skew_workload(), 4);

    assert_top_keys(
        &ranking,
        &[
            (b"zipf-0".as_slice(), 12, 12, 0),
            (b"zipf-1".as_slice(), 6, 6, 0),
            (b"zipf-2".as_slice(), 3, 3, 0),
            (b"zipf-3".as_slice(), 1, 1, 0),
        ],
    );
}

#[test]
fn phase2a_matrix_temporal_shift_changes_the_hot_key_over_time() {
    let first_window = evaluate_workload(temporal_shift_workload_first_window(), 2);
    let second_window = evaluate_workload(temporal_shift_workload_second_window(), 2);

    assert_top_keys(
        &first_window,
        &[
            (b"shift-a".as_slice(), 8, 8, 0),
            (b"shift-b".as_slice(), 2, 2, 0),
        ],
    );
    assert_top_keys(
        &second_window,
        &[
            (b"shift-b".as_slice(), 8, 8, 0),
            (b"shift-a".as_slice(), 2, 2, 0),
        ],
    );
}

#[test]
fn phase2a_matrix_bursty_spikes_distinguishes_short_lived_surge_from_background() {
    let ranking = evaluate_workload(bursty_spike_workload(), 3);

    assert_top_keys_in_current_evaluator_order(
        &ranking,
        &[
            (b"spike".as_slice(), 10, 10, 0),
            (b"background".as_slice(), 4, 4, 0),
            (b"baseline-a".as_slice(), 4, 4, 0),
        ],
    );
}

#[test]
fn phase2a_matrix_near_uniform_signal_stays_tightly_grouped() {
    let ranking = evaluate_workload(near_uniform_weak_signal_workload(), 4);

    assert_top_keys_in_current_evaluator_order(
        &ranking,
        &[
            (b"weak-a".as_slice(), 3, 3, 0),
            (b"weak-b".as_slice(), 3, 3, 0),
            (b"weak-c".as_slice(), 3, 3, 0),
            (b"weak-d".as_slice(), 2, 2, 0),
        ],
    );
}

#[test]
fn phase2a_stability_sustained_skew_stays_consistent_across_repeated_captures() {
    let report = run_sustained_stability_harness(stable_sustained_skew_windows(3), 3, 3);

    assert_eq!(report.current_window_events, 0);
    assert_window_state_cleared(&report.captures, 40);
    assert_stability_capture(
        &report.captures[0],
        &[
            (b"stable-hot".as_slice(), 24),
            (b"steady-warm".as_slice(), 8),
            (b"cold-a".as_slice(), 4),
        ],
    );
    assert_stability_capture(
        &report.captures[1],
        &[
            (b"stable-hot".as_slice(), 48),
            (b"steady-warm".as_slice(), 16),
            (b"cold-a".as_slice(), 8),
        ],
    );
    assert_stability_capture(
        &report.captures[2],
        &[
            (b"stable-hot".as_slice(), 72),
            (b"steady-warm".as_slice(), 24),
            (b"cold-a".as_slice(), 12),
        ],
    );
}

#[test]
fn phase2a_stability_retains_only_recent_captures_and_clears_window_state() {
    let report = run_sustained_stability_harness(stable_sustained_skew_windows(5), 2, 5);

    assert_eq!(report.current_window_events, 0);
    assert_eq!(report.captures.len(), 2);
    assert_eq!(report.total_events_seen, 200);
    assert_window_state_cleared(&report.captures, 40);
    assert_stability_capture(
        &report.captures[0],
        &[
            (b"stable-hot".as_slice(), 96),
            (b"steady-warm".as_slice(), 32),
            (b"cold-a".as_slice(), 16),
            (b"cold-b".as_slice(), 16),
        ],
    );
    assert_stability_capture(
        &report.captures[1],
        &[
            (b"stable-hot".as_slice(), 120),
            (b"steady-warm".as_slice(), 40),
            (b"cold-a".as_slice(), 20),
            (b"cold-b".as_slice(), 20),
        ],
    );
}

fn observed_read(command: CommandKind, key: &[u8]) -> ObservationEvent {
    ObservationEvent::read(command, key.to_vec(), std::time::UNIX_EPOCH)
}

fn run_sustained_stability_harness(
    windows: Vec<Vec<ObservationEvent>>,
    retained_captures: usize,
    top_keys_limit: usize,
) -> SustainedStabilityReport {
    let mut evaluator = ExactHotnessEvaluator::default();
    let mut captures = Vec::new();
    let mut total_events_seen = 0;
    let mut window_accumulator = WindowAccumulator::default();

    for (capture_index, window) in windows.into_iter().enumerate() {
        for event in window {
            window_accumulator.record_event();
            evaluator.record(event);
            total_events_seen += 1;
        }

        let window_events_before_reset = window_accumulator.event_count();
        let reset_from_event_count = window_accumulator.reset();
        captures.push(StabilityCapture {
            capture_index,
            window_events_before_reset,
            reset_from_event_count,
            window_events_after_reset: window_accumulator.event_count(),
            cumulative_top_keys: evaluator.top_keys(top_keys_limit),
        });

        if captures.len() > retained_captures {
            let overflow = captures.len() - retained_captures;
            captures.drain(0..overflow);
        }
    }

    SustainedStabilityReport {
        total_events_seen,
        current_window_events: window_accumulator.event_count(),
        captures,
    }
}

fn evaluate_workload(events: Vec<ObservationEvent>, limit: usize) -> Vec<ExactHotKey> {
    ExactHotnessEvaluator::from_events(events).top_keys(limit)
}

fn assert_stability_capture(capture: &StabilityCapture, expected: &[(&[u8], u64)]) {
    let expected: Vec<_> = expected
        .iter()
        .map(|(key, total_accesses)| ExactHotKey {
            key: key.to_vec(),
            total_accesses: *total_accesses,
            read_accesses: *total_accesses,
            write_accesses: 0,
        })
        .collect();

    assert_eq!(capture.cumulative_top_keys, expected);
}

fn assert_window_state_cleared(captures: &[StabilityCapture], expected_window_events: usize) {
    for capture in captures {
        assert_eq!(capture.window_events_before_reset, expected_window_events);
        assert_eq!(capture.reset_from_event_count, expected_window_events);
        assert_eq!(capture.window_events_after_reset, 0);
    }
}

fn assert_top_keys(actual: &[ExactHotKey], expected: &[(&[u8], u64, u64, u64)]) {
    let expected: Vec<_> = expected
        .iter()
        .map(
            |(key, total_accesses, read_accesses, write_accesses)| ExactHotKey {
                key: key.to_vec(),
                total_accesses: *total_accesses,
                read_accesses: *read_accesses,
                write_accesses: *write_accesses,
            },
        )
        .collect();

    assert_eq!(actual, expected);
}

fn assert_top_keys_in_current_evaluator_order(
    actual: &[ExactHotKey],
    expected: &[(&[u8], u64, u64, u64)],
) {
    let expected: Vec<_> = expected
        .iter()
        .map(
            |(key, total_accesses, read_accesses, write_accesses)| ExactHotKey {
                key: key.to_vec(),
                total_accesses: *total_accesses,
                read_accesses: *read_accesses,
                write_accesses: *write_accesses,
            },
        )
        .collect();

    assert_eq!(
        actual,
        expected,
        "ordering follows current evaluator semantics: total accesses, then read/write split, then key bytes"
    );
}

fn zipf_like_skew_workload() -> Vec<ObservationEvent> {
    repeated_reads(b"zipf-0", 12)
        .into_iter()
        .chain(repeated_reads(b"zipf-1", 6))
        .chain(repeated_reads(b"zipf-2", 3))
        .chain(repeated_reads(b"zipf-3", 1))
        .collect()
}

fn temporal_shift_workload_first_window() -> Vec<ObservationEvent> {
    repeated_reads(b"shift-a", 8)
        .into_iter()
        .chain(repeated_reads(b"shift-b", 2))
        .chain(repeated_reads(b"steady", 2))
        .collect()
}

fn temporal_shift_workload_second_window() -> Vec<ObservationEvent> {
    repeated_reads(b"shift-b", 8)
        .into_iter()
        .chain(repeated_reads(b"shift-a", 2))
        .chain(repeated_reads(b"steady", 2))
        .collect()
}

fn bursty_spike_workload() -> Vec<ObservationEvent> {
    repeated_reads(b"baseline-a", 2)
        .into_iter()
        .chain(repeated_reads(b"baseline-b", 2))
        .chain(repeated_reads(b"background", 2))
        .chain(repeated_reads(b"spike", 10))
        .chain(repeated_reads(b"baseline-a", 2))
        .chain(repeated_reads(b"baseline-b", 2))
        .chain(repeated_reads(b"background", 2))
        .collect()
}

fn near_uniform_weak_signal_workload() -> Vec<ObservationEvent> {
    repeated_reads(b"weak-a", 3)
        .into_iter()
        .chain(repeated_reads(b"weak-b", 3))
        .chain(repeated_reads(b"weak-c", 3))
        .chain(repeated_reads(b"weak-d", 2))
        .chain(repeated_reads(b"weak-e", 2))
        .collect()
}

fn stable_sustained_skew_windows(window_count: usize) -> Vec<Vec<ObservationEvent>> {
    (0..window_count).map(|_| sustained_skew_window()).collect()
}

fn sustained_skew_window() -> Vec<ObservationEvent> {
    repeated_reads(b"stable-hot", 24)
        .into_iter()
        .chain(repeated_reads(b"steady-warm", 8))
        .chain(repeated_reads(b"cold-a", 4))
        .chain(repeated_reads(b"cold-b", 4))
        .collect()
}

fn repeated_reads(key: &[u8], count: usize) -> Vec<ObservationEvent> {
    (0..count)
        .map(|_| observed_read(CommandKind::Get, key))
        .collect()
}
