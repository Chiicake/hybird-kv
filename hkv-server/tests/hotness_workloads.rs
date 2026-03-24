use hkv_server::phase2a_testing::{
    CommandKind, ExactHotKey, ExactHotnessEvaluator, ObservationEvent,
};

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

fn observed_read(command: CommandKind, key: &[u8]) -> ObservationEvent {
    ObservationEvent::read(command, key.to_vec(), std::time::UNIX_EPOCH)
}

fn evaluate_workload(events: Vec<ObservationEvent>, limit: usize) -> Vec<ExactHotKey> {
    ExactHotnessEvaluator::from_events(events).top_keys(limit)
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

fn repeated_reads(key: &[u8], count: usize) -> Vec<ObservationEvent> {
    (0..count)
        .map(|_| observed_read(CommandKind::Get, key))
        .collect()
}
