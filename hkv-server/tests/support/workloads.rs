use hkv_server::phase2a_testing::{CommandKind, ObservationEvent};

pub(crate) fn zipf_like_skew_workload() -> Vec<ObservationEvent> {
    repeated_reads(b"zipf-0", 12)
        .into_iter()
        .chain(repeated_reads(b"zipf-1", 6))
        .chain(repeated_reads(b"zipf-2", 3))
        .chain(repeated_reads(b"zipf-3", 1))
        .collect()
}

pub(crate) fn temporal_shift_workload_first_window() -> Vec<ObservationEvent> {
    repeated_reads(b"shift-a", 8)
        .into_iter()
        .chain(repeated_reads(b"shift-b", 2))
        .chain(repeated_reads(b"steady", 2))
        .collect()
}

pub(crate) fn temporal_shift_workload_second_window() -> Vec<ObservationEvent> {
    repeated_reads(b"shift-b", 8)
        .into_iter()
        .chain(repeated_reads(b"shift-a", 2))
        .chain(repeated_reads(b"steady", 2))
        .collect()
}

pub(crate) fn bursty_spike_workload() -> Vec<ObservationEvent> {
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

pub(crate) fn near_uniform_weak_signal_workload() -> Vec<ObservationEvent> {
    repeated_reads(b"weak-a", 3)
        .into_iter()
        .chain(repeated_reads(b"weak-b", 3))
        .chain(repeated_reads(b"weak-c", 3))
        .chain(repeated_reads(b"weak-d", 2))
        .chain(repeated_reads(b"weak-e", 2))
        .collect()
}

pub(crate) fn steady_rollover_probe_event() -> ObservationEvent {
    ObservationEvent::read(
        CommandKind::Get,
        b"rollover-probe".to_vec(),
        std::time::UNIX_EPOCH,
    )
}

#[allow(dead_code)]
pub(crate) fn stable_sustained_skew_windows(window_count: usize) -> Vec<Vec<ObservationEvent>> {
    (0..window_count).map(|_| sustained_skew_window()).collect()
}

#[allow(dead_code)]
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
        .map(|_| ObservationEvent::read(CommandKind::Get, key.to_vec(), std::time::UNIX_EPOCH))
        .collect()
}
