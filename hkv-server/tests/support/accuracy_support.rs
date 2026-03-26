use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hkv_client::KVClient;
use hkv_server::phase2a_testing::{ExactHotKey, ExactHotnessEvaluator, ObservationEvent};
use hkv_server::tracker::{HotCandidate, HotTracker};

pub(crate) fn exact_top_keys(events: Vec<ObservationEvent>, top_n: usize) -> Vec<ExactHotKey> {
    ExactHotnessEvaluator::from_events(events).top_keys(top_n)
}

pub(crate) fn top_n_overlap(exact: &[ExactHotKey], snapshot: &[HotCandidate], top_n: usize) -> usize {
    let exact: HashSet<_> = exact.iter().take(top_n).map(|key| key.key.clone()).collect();
    let snapshot: HashSet<_> = snapshot.iter().take(top_n).map(|key| key.key.clone()).collect();
    exact.intersection(&snapshot).count()
}

pub(crate) fn top_n_exact_keys(exact: &[ExactHotKey], top_n: usize) -> Vec<Vec<u8>> {
    exact.iter().take(top_n).map(|key| key.key.clone()).collect()
}

pub(crate) fn top_n_keys(snapshot: &[HotCandidate], top_n: usize) -> Vec<Vec<u8>> {
    snapshot.iter().take(top_n).map(|key| key.key.clone()).collect()
}

pub(crate) fn distinct_exact_frequency_bands(exact: &[ExactHotKey]) -> Vec<u64> {
    let mut counts: Vec<_> = exact.iter().map(|key| key.total_accesses).collect();
    counts.sort_unstable_by(|left, right| right.cmp(left));
    counts.dedup();
    counts
}

pub(crate) fn read_snapshot_candidates(client: &KVClient) -> Vec<HotCandidate> {
    let info = String::from_utf8(client.info().unwrap()).unwrap();
    let mut indexed = std::collections::BTreeMap::<usize, HotCandidate>::new();

    for line in info.lines() {
        if let Some((prefix, value)) = line.split_once(':') {
            if let Some(index) = prefix
                .strip_prefix("hot_candidate_")
                .and_then(|suffix| suffix.split_once('_'))
                .and_then(|(index, field)| index.parse::<usize>().ok().map(|index| (index, field)))
            {
                let candidate = indexed.entry(index.0).or_insert_with(|| HotCandidate {
                    key: Vec::new(),
                    estimated_total_accesses: 0,
                    estimated_read_accesses: 0,
                    last_known_value_size: None,
                    ineligible_reason: None,
                });
                match index.1 {
                    "key_hex" => candidate.key = decode_hex(value),
                    "total_accesses" => candidate.estimated_total_accesses = value.parse().unwrap(),
                    "read_accesses" => candidate.estimated_read_accesses = value.parse().unwrap(),
                    _ => {}
                }
            }
        }
    }

    indexed.into_values().collect()
}

pub(crate) async fn wait_for_non_empty_snapshot(
    tracker: &Arc<Mutex<HotTracker>>,
) -> Result<Vec<HotCandidate>, String> {
    let start = std::time::Instant::now();
    loop {
        let snapshot = tracker.lock().unwrap().latest_snapshot().candidates;
        if !snapshot.is_empty() {
            return Ok(snapshot);
        }
        if start.elapsed() > Duration::from_secs(1) {
            return Err("timed out waiting for non-empty snapshot".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub(crate) async fn wait_for_snapshot_head(
    tracker: &Arc<Mutex<HotTracker>>,
    expected_head: &[u8],
) -> Result<Vec<HotCandidate>, String> {
    let start = std::time::Instant::now();
    loop {
        let snapshot = tracker.lock().unwrap().latest_snapshot().candidates;
        if snapshot.first().map(|candidate| candidate.key.as_slice()) == Some(expected_head) {
            return Ok(snapshot);
        }
        if start.elapsed() > Duration::from_secs(1) {
            return Err(format!(
                "timed out waiting for snapshot head {}; last snapshot: {:?}",
                String::from_utf8_lossy(expected_head),
                snapshot
            ));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(text, 16).unwrap()
        })
        .collect()
}
