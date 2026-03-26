use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hkv_client::KVClient;
use hkv_engine::MemoryEngine;
use hkv_server::metrics::Metrics;
use hkv_server::phase2a_testing::{CommandKind, ExactHotKey, ExactHotnessEvaluator, ObservationEvent};
use hkv_server::server;
use hkv_server::tracker::{HotCandidate, HotTracker, TrackerConfig};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub(crate) async fn spawn_tracker_server(
    config: TrackerConfig,
) -> std::io::Result<(SocketAddr, Arc<Mutex<HotTracker>>, oneshot::Sender<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let engine = Arc::new(MemoryEngine::new());
    let metrics = Arc::new(Metrics::new());
    let tracker = Arc::new(Mutex::new(HotTracker::new(config)));
    let expirer = engine.start_expirer(Duration::from_millis(50));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_metrics = Arc::clone(&metrics);
    let server_tracker = Arc::clone(&tracker);

    tokio::spawn(async move {
        let mut expirer = Some(expirer);
        let _ = server::serve_with_shutdown_and_tracker(
            listener,
            engine,
            server_metrics,
            server_tracker,
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await;

        if let Some(handle) = expirer.take() {
            handle.stop();
        }
    });

    Ok((addr, tracker, shutdown_tx))
}

pub(crate) fn default_tracker_config(window_duration: Duration) -> TrackerConfig {
    TrackerConfig {
        candidate_limit: 8,
        max_value_size: 1024,
        registry_capacity: 64,
        max_key_bytes: 256,
        cms_width: 128,
        cms_depth: 4,
        window_duration,
        min_recent_accesses: 1,
        min_read_ratio_percent: 0,
        max_idle_age: Duration::from_secs(120),
    }
}

pub(crate) fn apply_event(client: &KVClient, event: &ObservationEvent) {
    match event.command {
        CommandKind::Get | CommandKind::Ttl => {
            let _ = client.get(&event.key).unwrap();
        }
        CommandKind::Set => {
            let value = value_bytes_for(event);
            client.set(&event.key, &value).unwrap();
        }
        CommandKind::Delete => {
            let _ = client.delete(&event.key).unwrap();
        }
        CommandKind::Expire => {
            let ttl = event_ttl(event);
            let _ = client.expire(&event.key, ttl).unwrap();
        }
        CommandKind::Unknown => {}
    }
}

#[allow(dead_code)]
pub(crate) fn exact_top_keys(events: Vec<ObservationEvent>, top_n: usize) -> Vec<ExactHotKey> {
    ExactHotnessEvaluator::from_events(events).top_keys(top_n)
}

#[allow(dead_code)]
pub(crate) fn top_n_overlap(exact: &[ExactHotKey], snapshot: &[HotCandidate], top_n: usize) -> usize {
    use std::collections::HashSet;
    let exact: HashSet<_> = exact.iter().take(top_n).map(|key| key.key.clone()).collect();
    let snapshot: HashSet<_> = snapshot.iter().take(top_n).map(|key| key.key.clone()).collect();
    exact.intersection(&snapshot).count()
}

#[allow(dead_code)]
pub(crate) fn distinct_exact_frequency_bands(exact: &[ExactHotKey]) -> Vec<u64> {
    let mut counts: Vec<_> = exact.iter().map(|key| key.total_accesses).collect();
    counts.sort_unstable_by(|left, right| right.cmp(left));
    counts.dedup();
    counts
}

#[allow(dead_code)]
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

fn event_ttl(event: &ObservationEvent) -> Duration {
    event.timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(1))
        .max(Duration::from_secs(1))
}

fn value_bytes_for(event: &ObservationEvent) -> Vec<u8> {
    vec![b'x'; event.value_size.unwrap_or(1)]
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
