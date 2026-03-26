#[path = "support/accuracy_support.rs"]
mod accuracy;
#[path = "support/hotness_workload_support.rs"]
mod harness;
#[path = "support/workloads.rs"]
mod workloads;

use std::collections::HashSet;
use std::time::Duration;

use hkv_client::KVClient;
use hkv_server::phase2a_testing::{ExactHotKey, ObservationEvent};
use hkv_server::tracker::HotCandidate;

#[derive(Debug)]
struct AccuracyRun {
    exact: Vec<ExactHotKey>,
    snapshot: Vec<HotCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExplicitMismatch {
    exact_only: Vec<Vec<u8>>,
    tracker_only: Vec<Vec<u8>>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skewed_heavy_hitters_overlap_exact_top_n() {
    let run = replay_workload_against_server(workloads::zipf_like_skew_workload(), 4).await;

    assert_eq!(accuracy::top_n_overlap(&run.exact, &run.snapshot, 4), 4, "{run:#?}");
    assert_eq!(accuracy::top_n_keys(&run.snapshot, 4), accuracy::top_n_exact_keys(&run.exact, 4), "{run:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn temporal_shift_prefers_the_same_head_key_as_exact_ground_truth() {
    let config = harness::default_tracker_config(Duration::from_millis(200));
    let (addr, tracker, shutdown) = harness::spawn_tracker_server(config).await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    let first_events = workloads::temporal_shift_workload_first_window();
    let first_exact_head = accuracy::exact_top_keys(first_events.clone(), 1)[0].key.clone();
    for event in &first_events {
        harness::apply_event(&client, event);
    }
    let first_snapshot = accuracy::wait_for_snapshot_head(&tracker, &first_exact_head)
        .await
        .unwrap();

    let second_events = workloads::temporal_shift_workload_second_window();
    let second_exact_head = accuracy::exact_top_keys(second_events.clone(), 1)[0].key.clone();
    let mut rollover_probe_events = Vec::new();
    let rollover_start = std::time::Instant::now();
    while rollover_start.elapsed() < Duration::from_secs(1) {
        let probe = workloads::steady_rollover_probe_event();
        harness::apply_event(&client, &probe);
        rollover_probe_events.push(probe);
        let snapshot = accuracy::read_snapshot_candidates(&client);
        if snapshot.first().map(|candidate| candidate.key.as_slice()) != Some(first_snapshot[0].key.as_slice()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for event in &second_events {
        harness::apply_event(&client, event);
    }
    let second_snapshot = accuracy::wait_for_snapshot_head(&tracker, &second_exact_head)
        .await
        .unwrap();

    let _ = shutdown.send(());

    let first_exact = accuracy::exact_top_keys(first_events, 2);
    let mut second_exact_events = rollover_probe_events;
    second_exact_events.extend(second_events);
    let second_exact = accuracy::exact_top_keys(second_exact_events, 2);

    assert_eq!(first_snapshot[0].key, first_exact[0].key, "first={first_snapshot:#?}");
    assert_eq!(second_snapshot[0].key, second_exact[0].key, "second={second_snapshot:#?}");
    assert_ne!(first_snapshot[0].key, second_snapshot[0].key);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bursty_spike_matches_exact_top_n_with_explicit_mismatch_report() {
    let run = replay_workload_against_server(workloads::bursty_spike_workload(), 3).await;

    assert_eq!(accuracy::top_n_overlap(&run.exact, &run.snapshot, 3), 2, "{run:#?}");
    assert_eq!(run.snapshot[0].key, run.exact[0].key, "{run:#?}");
    assert_eq!(run.snapshot[1].key, run.exact[1].key, "{run:#?}");

    let mismatch = explicit_mismatch(&run.exact, &run.snapshot, 3);
    assert_eq!(mismatch.exact_only.len(), 1, "{run:#?}");
    assert_eq!(mismatch.tracker_only.len(), 1, "{run:#?}");
    assert!(
        [b"baseline-a".to_vec(), b"baseline-b".to_vec()].contains(&mismatch.exact_only[0]),
        "{run:#?}"
    );
    assert!(
        [b"baseline-a".to_vec(), b"baseline-b".to_vec()].contains(&mismatch.tracker_only[0]),
        "{run:#?}"
    );
    assert_ne!(mismatch.exact_only[0], mismatch.tracker_only[0], "{run:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn weak_signal_ordering_matches_exact_when_counts_are_distinct() {
    let run = replay_workload_against_server(workloads::near_uniform_weak_signal_workload(), 5).await;

    assert!(ordering_matches_exact_distinct_groups(&run.exact, &run.snapshot), "{run:#?}");
    assert_eq!(accuracy::top_n_overlap(&run.exact, &run.snapshot, 3), 3, "{run:#?}");

    let tracker_keys = accuracy::top_n_keys(&run.snapshot, 5);
    let exact_top3: HashSet<_> = accuracy::top_n_exact_keys(&run.exact, 3).into_iter().collect();
    let tied_tail: HashSet<_> = [b"weak-d".to_vec(), b"weak-e".to_vec()].into_iter().collect();

    for key in tracker_keys.iter().take(3) {
        assert!(exact_top3.contains(key), "{run:#?}");
    }
    for key in tracker_keys.iter().skip(3) {
        assert!(tied_tail.contains(key), "{run:#?}");
    }
}

async fn replay_workload_against_server(events: Vec<ObservationEvent>, top_n: usize) -> AccuracyRun {
    let (addr, tracker, shutdown) = harness::spawn_tracker_server(harness::default_tracker_config(Duration::from_secs(30))).await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    let unique_keys: HashSet<Vec<u8>> = events.iter().map(|event| event.key.clone()).collect();
    for key in unique_keys {
        client.set(&key, b"seed").unwrap();
    }

    for event in &events {
        harness::apply_event(&client, event);
    }

    let snapshot = accuracy::wait_for_non_empty_snapshot(&tracker).await.unwrap();
    let _ = shutdown.send(());

    AccuracyRun {
        exact: accuracy::exact_top_keys(events, top_n),
        snapshot,
    }
}

fn ordering_matches_exact_distinct_groups(exact: &[ExactHotKey], snapshot: &[HotCandidate]) -> bool {
    let distinct_counts = accuracy::distinct_exact_frequency_bands(exact);

    distinct_counts.windows(2).all(|pair| {
        let higher_group: HashSet<Vec<u8>> = exact
            .iter()
            .filter(|candidate| candidate.total_accesses == pair[0])
            .map(|candidate| candidate.key.clone())
            .collect();
        let lower_group: HashSet<Vec<u8>> = exact
            .iter()
            .filter(|candidate| candidate.total_accesses == pair[1])
            .map(|candidate| candidate.key.clone())
            .collect();

        let highest_lower_index = snapshot
            .iter()
            .enumerate()
            .filter(|(_, candidate)| lower_group.contains(&candidate.key))
            .map(|(index, _)| index)
            .min()
            .unwrap_or(usize::MAX);
        let lowest_higher_index = snapshot
            .iter()
            .enumerate()
            .filter(|(_, candidate)| higher_group.contains(&candidate.key))
            .map(|(index, _)| index)
            .max()
            .unwrap_or(usize::MAX);

        lowest_higher_index != usize::MAX
            && highest_lower_index != usize::MAX
            && lowest_higher_index < highest_lower_index
    })
}

fn explicit_mismatch(exact: &[ExactHotKey], snapshot: &[HotCandidate], top_n: usize) -> ExplicitMismatch {
    let exact_keys = accuracy::top_n_exact_keys(exact, top_n);
    let snapshot_keys = accuracy::top_n_keys(snapshot, top_n);
    let snapshot_set: HashSet<_> = snapshot_keys.iter().cloned().collect();
    let exact_set: HashSet<_> = exact_keys.iter().cloned().collect();

    ExplicitMismatch {
        exact_only: exact_keys.into_iter().filter(|key| !snapshot_set.contains(key)).collect(),
        tracker_only: snapshot_keys.into_iter().filter(|key| !exact_set.contains(key)).collect(),
    }
}
