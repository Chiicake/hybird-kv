use std::io::{Read, Write};
use std::mem::size_of;
use std::net::{Shutdown, SocketAddr, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hkv_client::{ClientTtl, KVClient};
use hkv_engine::MemoryEngine;
use hkv_server::metrics::Metrics;
#[path = "support/hotness_workload_support.rs"]
mod harness;

use hkv_server::phase2a_testing::{AccessClass, CommandKind, SharedObservationLog};
use hkv_server::server;
use hkv_server::tracker::{HotTracker, TrackerConfig};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn spawn_test_server()
-> std::io::Result<(SocketAddr, Arc<SharedObservationLog>, oneshot::Sender<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let engine = Arc::new(MemoryEngine::new());
    let metrics = Arc::new(Metrics::new());
    let observation_log = Arc::new(SharedObservationLog::default());
    let expirer = engine.start_expirer(Duration::from_millis(50));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_metrics = Arc::clone(&metrics);
    let server_observation_log = Arc::clone(&observation_log);

    tokio::spawn(async move {
        let mut expirer = Some(expirer);
        let _ = server::serve_with_shutdown_and_observation(
            listener,
            engine,
            server_metrics,
            server_observation_log,
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await;

        if let Some(handle) = expirer.take() {
            handle.stop();
        }
    });

    Ok((addr, observation_log, shutdown_tx))
}

async fn spawn_baseline_server() -> std::io::Result<(SocketAddr, oneshot::Sender<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let engine = Arc::new(MemoryEngine::new());
    let metrics = Arc::new(Metrics::new());
    let expirer = engine.start_expirer(Duration::from_millis(50));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_metrics = Arc::clone(&metrics);

    tokio::spawn(async move {
        let mut expirer = Some(expirer);
        let _ = server::serve_with_shutdown(listener, engine, server_metrics, async {
            let _ = shutdown_rx.await;
        })
        .await;

        if let Some(handle) = expirer.take() {
            handle.stop();
        }
    });

    Ok((addr, shutdown_tx))
}

fn send_raw(addr: SocketAddr, request: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut stream = StdTcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(request)?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(response)
}

#[derive(Debug, Clone, Copy)]
struct OverheadWorkloadConfig {
    workers: usize,
    rounds_per_worker: usize,
}

#[derive(Debug, Clone, Copy)]
struct OverheadWorkflowConfig {
    workload: OverheadWorkloadConfig,
    warmup_runs: usize,
    measured_runs: usize,
}

impl OverheadWorkflowConfig {
    fn total_ops_per_run(self) -> usize {
        self.workload.workers * self.workload.rounds_per_worker * 6
    }
}

#[derive(Debug, Clone, Copy)]
struct OverheadRunMetrics {
    total_ops: usize,
    elapsed: Duration,
    ops_per_sec: f64,
    avg_ns_per_op: f64,
    max_worker_elapsed: Duration,
    worker_skew_ratio: f64,
}

#[derive(Debug, Clone, Copy)]
struct OverheadRunSummary {
    total_ops: usize,
    measured_runs: usize,
    avg_elapsed: Duration,
    avg_ops_per_sec: f64,
    avg_ns_per_op: f64,
    avg_max_worker_elapsed: Duration,
    avg_worker_skew_ratio: f64,
}

#[derive(Debug, Clone, Copy)]
struct ObservationCountSummary {
    warmup_events: usize,
    measured_events: usize,
}

#[derive(Debug, Clone, Copy)]
struct OverheadComparison {
    latency_delta_pct: f64,
    throughput_delta_pct: f64,
    runtime_delta_pct: f64,
}

#[derive(Debug, Clone, Copy)]
struct TrackerOverheadReport {
    warmup_snapshot_publishes: usize,
    measured_snapshot_publishes: usize,
    latest_candidate_count: usize,
    latest_observed_total_accesses: u64,
}

async fn spawn_tracker_server()
-> std::io::Result<(SocketAddr, Arc<Mutex<HotTracker>>, oneshot::Sender<()>)> {
    harness::spawn_tracker_server(harness::default_tracker_config(Duration::from_secs(30))).await
}

fn capture_tracker_overhead_report(
    tracker: &Arc<Mutex<HotTracker>>,
    config: OverheadWorkflowConfig,
) -> TrackerOverheadReport {
    let snapshot = tracker.lock().unwrap().latest_snapshot();

    TrackerOverheadReport {
        warmup_snapshot_publishes: config.warmup_runs,
        measured_snapshot_publishes: config.measured_runs,
        latest_candidate_count: snapshot.candidates.len(),
        latest_observed_total_accesses: snapshot.observed_total_accesses,
    }
}

async fn run_overhead_workload(
    addr: SocketAddr,
    config: OverheadWorkloadConfig,
) -> OverheadRunMetrics {
    let started = Instant::now();
    let mut tasks = Vec::with_capacity(config.workers);

    for worker in 0..config.workers {
        tasks.push(tokio::task::spawn_blocking(move || {
            let client = KVClient::connect(addr.to_string()).unwrap();
            let worker_started = Instant::now();

            for round in 0..config.rounds_per_worker {
                let key = format!("overhead:{worker}:{round}");
                let value = format!("value:{worker}:{round}");
                client.set(key.as_bytes(), value.as_bytes()).unwrap();
                assert_eq!(
                    client.get(key.as_bytes()).unwrap(),
                    Some(value.into_bytes())
                );
                assert!(
                    client
                        .expire(key.as_bytes(), Duration::from_secs(30))
                        .unwrap()
                );
                assert!(matches!(
                    client.ttl(key.as_bytes()).unwrap(),
                    ClientTtl::ExpiresIn(_)
                ));
                assert!(client.delete(key.as_bytes()).unwrap());
                assert_eq!(client.get(key.as_bytes()).unwrap(), None);
            }

            worker_started.elapsed()
        }));
    }

    let mut worker_elapsed_sum = Duration::ZERO;
    let mut max_worker_elapsed = Duration::ZERO;
    for task in tasks {
        let worker_elapsed = task.await.unwrap();
        worker_elapsed_sum += worker_elapsed;
        max_worker_elapsed = max_worker_elapsed.max(worker_elapsed);
    }

    let elapsed = started.elapsed();
    let total_ops = config.workers * config.rounds_per_worker * 6;
    let elapsed_secs = elapsed.as_secs_f64();
    let mean_worker_secs = worker_elapsed_sum.as_secs_f64() / config.workers as f64;

    OverheadRunMetrics {
        total_ops,
        elapsed,
        ops_per_sec: total_ops as f64 / elapsed_secs,
        avg_ns_per_op: elapsed.as_nanos() as f64 / total_ops as f64,
        max_worker_elapsed,
        worker_skew_ratio: if mean_worker_secs == 0.0 {
            1.0
        } else {
            max_worker_elapsed.as_secs_f64() / mean_worker_secs
        },
    }
}

async fn run_overhead_series(
    addr: SocketAddr,
    config: OverheadWorkflowConfig,
) -> Vec<OverheadRunMetrics> {
    for _ in 0..config.warmup_runs {
        let _ = run_overhead_workload(addr, config.workload).await;
    }

    let mut measured = Vec::with_capacity(config.measured_runs);
    for _ in 0..config.measured_runs {
        measured.push(run_overhead_workload(addr, config.workload).await);
    }
    measured
}

fn summarize_runs(runs: &[OverheadRunMetrics]) -> OverheadRunSummary {
    assert!(!runs.is_empty());

    let total_ops = runs[0].total_ops;
    let measured_runs = runs.len();
    let elapsed_secs = runs
        .iter()
        .map(|run| run.elapsed.as_secs_f64())
        .sum::<f64>()
        / measured_runs as f64;
    let avg_ops_per_sec =
        runs.iter().map(|run| run.ops_per_sec).sum::<f64>() / measured_runs as f64;
    let avg_ns_per_op =
        runs.iter().map(|run| run.avg_ns_per_op).sum::<f64>() / measured_runs as f64;
    let avg_max_worker_secs = runs
        .iter()
        .map(|run| run.max_worker_elapsed.as_secs_f64())
        .sum::<f64>()
        / measured_runs as f64;
    let avg_worker_skew_ratio =
        runs.iter().map(|run| run.worker_skew_ratio).sum::<f64>() / measured_runs as f64;

    OverheadRunSummary {
        total_ops,
        measured_runs,
        avg_elapsed: Duration::from_secs_f64(elapsed_secs),
        avg_ops_per_sec,
        avg_ns_per_op,
        avg_max_worker_elapsed: Duration::from_secs_f64(avg_max_worker_secs),
        avg_worker_skew_ratio,
    }
}

fn compare_runs(baseline: OverheadRunSummary, observed: OverheadRunSummary) -> OverheadComparison {
    OverheadComparison {
        latency_delta_pct: percent_change(baseline.avg_ns_per_op, observed.avg_ns_per_op),
        throughput_delta_pct: percent_change(baseline.avg_ops_per_sec, observed.avg_ops_per_sec),
        runtime_delta_pct: percent_change(
            baseline.avg_elapsed.as_secs_f64(),
            observed.avg_elapsed.as_secs_f64(),
        ),
    }
}

fn percent_change(baseline: f64, observed: f64) -> f64 {
    if baseline == 0.0 {
        0.0
    } else {
        ((observed - baseline) / baseline) * 100.0
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_path_records_observation_events() {
    let (addr, observation_log, shutdown) = spawn_test_server().await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    client.set(b"obs:set", b"value").unwrap();
    assert_eq!(client.get(b"obs:set").unwrap(), Some(b"value".to_vec()));
    assert!(client.expire(b"obs:set", Duration::from_secs(10)).unwrap());
    assert!(matches!(
        client.ttl(b"obs:set").unwrap(),
        ClientTtl::ExpiresIn(_)
    ));
    client.delete(b"obs:set").unwrap();

    let unknown = send_raw(addr, b"*1\r\n$7\r\nUNKNOWN\r\n").unwrap();
    assert_eq!(unknown, b"-ERR unknown command\r\n");

    let malformed = send_raw(addr, b"*x\r\n").unwrap();
    assert_eq!(malformed, b"-ERR protocol error\r\n");

    let events = observation_log.observations();
    assert_eq!(events.len(), 5, "unexpected events: {events:#?}");

    assert_eq!(events[0].command, CommandKind::Set);
    assert_eq!(events[0].key, b"obs:set".to_vec());
    assert_eq!(events[0].access, AccessClass::Write);
    assert_eq!(events[0].value_size, Some(5));

    assert_eq!(events[1].command, CommandKind::Get);
    assert_eq!(events[1].key, b"obs:set".to_vec());
    assert_eq!(events[1].access, AccessClass::Read);
    assert_eq!(events[1].value_size, None);

    assert_eq!(events[2].command, CommandKind::Expire);
    assert_eq!(events[2].key, b"obs:set".to_vec());
    assert_eq!(events[2].access, AccessClass::Write);
    assert_eq!(events[2].value_size, None);

    assert_eq!(events[3].command, CommandKind::Ttl);
    assert_eq!(events[3].key, b"obs:set".to_vec());
    assert_eq!(events[3].access, AccessClass::Read);
    assert_eq!(events[3].value_size, None);

    assert_eq!(events[4].command, CommandKind::Delete);
    assert_eq!(events[4].key, b"obs:set".to_vec());
    assert_eq!(events[4].access, AccessClass::Write);
    assert_eq!(events[4].value_size, None);

    for event in &events {
        assert!(
            event
                .timestamp
                .duration_since(std::time::UNIX_EPOCH)
                .is_ok()
        );
    }

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_and_failed_commands_do_not_record_misleading_observation_events() {
    let (addr, observation_log, shutdown) = spawn_test_server().await.unwrap();

    let wrong_arity_get = send_raw(addr, b"*1\r\n$3\r\nGET\r\n").unwrap();
    assert_eq!(
        wrong_arity_get,
        b"-ERR wrong number of arguments for GET\r\n"
    );

    let wrong_arity_ttl = send_raw(addr, b"*1\r\n$3\r\nTTL\r\n").unwrap();
    assert_eq!(
        wrong_arity_ttl,
        b"-ERR wrong number of arguments for TTL\r\n"
    );

    let invalid_expire = send_raw(addr, b"*3\r\n$6\r\nEXPIRE\r\n$3\r\nkey\r\n$1\r\nx\r\n").unwrap();
    assert_eq!(invalid_expire, b"-ERR invalid integer\r\n");

    let invalid_set = send_raw(
        addr,
        b"*5\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n$2\r\nPX\r\n$1\r\n1\r\n",
    )
    .unwrap();
    assert_eq!(invalid_set, b"-ERR unsupported SET options\r\n");

    assert!(observation_log.observations().is_empty());

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overhead_workflow_reports_baseline_and_observed_server_runs() {
    let config = OverheadWorkflowConfig {
        workload: OverheadWorkloadConfig {
            workers: 4,
            rounds_per_worker: 32,
        },
        warmup_runs: 1,
        measured_runs: 2,
    };

    let (baseline_addr, baseline_shutdown) = spawn_baseline_server().await.unwrap();
    let baseline_runs = run_overhead_series(baseline_addr, config).await;
    let _ = baseline_shutdown.send(());

    let (observed_addr, observation_log, observed_shutdown) = spawn_test_server().await.unwrap();
    let observed_runs = run_overhead_series(observed_addr, config).await;
    let observed_events = observation_log.observations();
    let _ = observed_shutdown.send(());

    let baseline = summarize_runs(&baseline_runs);
    let observed = summarize_runs(&observed_runs);
    let comparison = compare_runs(baseline, observed);
    let observation_counts = ObservationCountSummary {
        warmup_events: config.total_ops_per_run() * config.warmup_runs,
        measured_events: config.total_ops_per_run() * config.measured_runs,
    };
    let event_storage_estimate_bytes =
        observed_events.len() * size_of::<hkv_server::phase2a_testing::ObservationEvent>();

    println!(
        "OVERHEAD_WORKFLOW workload workers={} rounds_per_worker={} warmup_runs={} measured_runs={} total_ops_per_run={}",
        config.workload.workers,
        config.workload.rounds_per_worker,
        config.warmup_runs,
        config.measured_runs,
        baseline.total_ops
    );
    println!(
        "OVERHEAD_WORKFLOW baseline avg_elapsed_ms={:.3} avg_ns_per_op={:.1} avg_ops_per_sec={:.1} avg_max_worker_ms={:.3} avg_worker_skew_ratio={:.3} measured_runs={}",
        baseline.avg_elapsed.as_secs_f64() * 1_000.0,
        baseline.avg_ns_per_op,
        baseline.avg_ops_per_sec,
        baseline.avg_max_worker_elapsed.as_secs_f64() * 1_000.0,
        baseline.avg_worker_skew_ratio,
        baseline.measured_runs,
    );
    println!(
        "OVERHEAD_WORKFLOW observed avg_elapsed_ms={:.3} avg_ns_per_op={:.1} avg_ops_per_sec={:.1} avg_max_worker_ms={:.3} avg_worker_skew_ratio={:.3} measured_runs={} warmup_observation_events={} measured_observation_events={} event_storage_estimate_bytes={}",
        observed.avg_elapsed.as_secs_f64() * 1_000.0,
        observed.avg_ns_per_op,
        observed.avg_ops_per_sec,
        observed.avg_max_worker_elapsed.as_secs_f64() * 1_000.0,
        observed.avg_worker_skew_ratio,
        observed.measured_runs,
        observation_counts.warmup_events,
        observation_counts.measured_events,
        event_storage_estimate_bytes,
    );
    println!(
        "OVERHEAD_WORKFLOW comparison latency_delta_pct={:.2} runtime_delta_pct={:.2} throughput_delta_pct={:.2}",
        comparison.latency_delta_pct, comparison.runtime_delta_pct, comparison.throughput_delta_pct,
    );

    assert_eq!(baseline_runs.len(), config.measured_runs);
    assert_eq!(observed_runs.len(), config.measured_runs);
    assert_eq!(baseline.total_ops, config.total_ops_per_run());
    assert_eq!(observed.total_ops, baseline.total_ops);
    assert!(baseline.avg_elapsed > Duration::ZERO);
    assert!(observed.avg_elapsed > Duration::ZERO);
    assert!(baseline.avg_ops_per_sec.is_finite() && baseline.avg_ops_per_sec > 0.0);
    assert!(observed.avg_ops_per_sec.is_finite() && observed.avg_ops_per_sec > 0.0);
    assert!(baseline.avg_ns_per_op.is_finite() && baseline.avg_ns_per_op > 0.0);
    assert!(observed.avg_ns_per_op.is_finite() && observed.avg_ns_per_op > 0.0);
    assert!(baseline.avg_worker_skew_ratio.is_finite() && baseline.avg_worker_skew_ratio >= 1.0);
    assert!(observed.avg_worker_skew_ratio.is_finite() && observed.avg_worker_skew_ratio >= 1.0);
    assert_eq!(
        observation_counts.warmup_events,
        observed.total_ops * config.warmup_runs
    );
    assert_eq!(
        observation_counts.measured_events,
        observed.total_ops * config.measured_runs
    );
    assert_eq!(
        observed_events.len(),
        observation_counts.warmup_events + observation_counts.measured_events
    );
    assert_eq!(
        event_storage_estimate_bytes,
        observed_events.len() * size_of::<hkv_server::phase2a_testing::ObservationEvent>()
    );
    assert_eq!(
        observed_events.first().map(|event| event.command),
        Some(CommandKind::Set)
    );
    assert_eq!(
        observed_events.last().map(|event| event.command),
        Some(CommandKind::Get)
    );
    assert!(comparison.latency_delta_pct.is_finite());
    assert!(comparison.runtime_delta_pct.is_finite());
    assert!(comparison.throughput_delta_pct.is_finite());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overhead_workflow_reports_candidate_export_signals_for_tracker_server_runs() {
    let config = OverheadWorkflowConfig {
        workload: OverheadWorkloadConfig {
            workers: 4,
            rounds_per_worker: 32,
        },
        warmup_runs: 1,
        measured_runs: 2,
    };

    let (baseline_addr, baseline_shutdown) = spawn_baseline_server().await.unwrap();
    let baseline_runs = run_overhead_series(baseline_addr, config).await;
    let _ = baseline_shutdown.send(());

    let (tracker_addr, tracker, tracker_shutdown) = spawn_tracker_server().await.unwrap();
    let tracker_runs = run_overhead_series(tracker_addr, config).await;
    let tracker_report = capture_tracker_overhead_report(&tracker, config);
    let _ = tracker_shutdown.send(());

    let baseline = summarize_runs(&baseline_runs);
    let tracker_summary = summarize_runs(&tracker_runs);
    let comparison = compare_runs(baseline, tracker_summary);

    println!(
        "CANDIDATE_EXPORT_OVERHEAD tracker_enabled=1 warmup_candidate_snapshots={} measured_candidate_snapshots={} candidate_count={} observed_total_accesses={} latency_delta_pct={:.2} runtime_delta_pct={:.2} throughput_delta_pct={:.2}",
        tracker_report.warmup_snapshot_publishes,
        tracker_report.measured_snapshot_publishes,
        tracker_report.latest_candidate_count,
        tracker_report.latest_observed_total_accesses,
        comparison.latency_delta_pct,
        comparison.runtime_delta_pct,
        comparison.throughput_delta_pct,
    );

    assert_eq!(baseline_runs.len(), config.measured_runs);
    assert_eq!(tracker_runs.len(), config.measured_runs);
    assert_eq!(tracker_report.warmup_snapshot_publishes, config.warmup_runs);
    assert_eq!(tracker_report.measured_snapshot_publishes, config.measured_runs);
    assert!(tracker_report.latest_candidate_count > 0, "{tracker_report:#?}");
    assert!(
        tracker_report.latest_observed_total_accesses >= config.total_ops_per_run() as u64,
        "{tracker_report:#?}"
    );
    assert!(comparison.latency_delta_pct.is_finite());
    assert!(comparison.runtime_delta_pct.is_finite());
    assert!(comparison.throughput_delta_pct.is_finite());
}
