use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;

use crate::models::{
    BenchmarkEventEnvelope, BenchmarkRun, BenchmarkRunRequest, NormalizedRunSummary,
    BENCHMARK_EVENT_CHANNEL,
};
use crate::runners::{
    select_runner, ActiveBenchmark, BenchmarkLifecycleEvent, BenchmarkRunner, RunnerError,
};

pub trait BenchmarkRunSink: Send + Sync {
    fn persist(&self, run: &BenchmarkRun) -> Result<(), String>;
}

pub struct NullBenchmarkRunSink;

impl BenchmarkRunSink for NullBenchmarkRunSink {
    fn persist(&self, _run: &BenchmarkRun) -> Result<(), String> {
        Ok(())
    }
}

pub struct BenchmarkManager {
    runners: Vec<Arc<dyn BenchmarkRunner>>,
    run_events: broadcast::Sender<BenchmarkRun>,
    lifecycle_events: broadcast::Sender<BenchmarkEventEnvelope>,
    sink: Arc<dyn BenchmarkRunSink>,
    state: Arc<Mutex<BenchmarkManagerState>>,
}

struct BenchmarkManagerState {
    active_runs: HashMap<String, ActiveRun>,
    runs: Vec<BenchmarkRun>,
}

struct ActiveRun {
    handle: Arc<dyn ActiveBenchmark>,
    run_index: usize,
}

impl BenchmarkManager {
    pub fn new(runners: Vec<Arc<dyn BenchmarkRunner>>) -> Self {
        Self::with_sink(runners, Arc::new(NullBenchmarkRunSink))
    }

    pub fn with_sink(
        runners: Vec<Arc<dyn BenchmarkRunner>>,
        sink: Arc<dyn BenchmarkRunSink>,
    ) -> Self {
        let (run_events, _) = broadcast::channel(32);
        let (lifecycle_events, _) = broadcast::channel(64);
        Self {
            runners,
            run_events,
            lifecycle_events,
            sink,
            state: Arc::new(Mutex::new(BenchmarkManagerState {
                active_runs: HashMap::new(),
                runs: Vec::new(),
            })),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BenchmarkRun> {
        self.run_events.subscribe()
    }

    pub fn subscribe_lifecycle(&self) -> broadcast::Receiver<BenchmarkEventEnvelope> {
        self.lifecycle_events.subscribe()
    }

    pub fn start(&self, request: BenchmarkRunRequest) -> Result<BenchmarkRun, RunnerError> {
        request
            .validate()
            .map_err(|message| RunnerError::new("invalid_request", message))?;

        let runner = select_runner(&self.runners, &request.runner)?;
        let run_id = next_run_id();
        let timestamp = iso_timestamp();
        let (sender, receiver) = mpsc::channel();
        let active: Arc<dyn ActiveBenchmark> = runner.start(request.clone(), sender)?.into();

        let run = BenchmarkRun {
            id: run_id.clone(),
            request,
            status: "running".into(),
            created_at: timestamp,
            started_at: Some(iso_timestamp()),
            finished_at: None,
            result: None,
            error_message: None,
        };

        {
            let mut state = self.state.lock().expect("benchmark manager mutex poisoned");
            let run_index = state.runs.len();
            state.runs.push(run.clone());
            state.active_runs.insert(
                run_id.clone(),
                ActiveRun {
                    handle: active,
                    run_index,
                },
            );
        }

        let _ = self.run_events.send(run.clone());
        let _ = self.lifecycle_events.send(lifecycle_envelope(
            &run.id,
            "started",
            Some(format!("benchmark run {} started", run.id)),
            None,
        ));
        self.spawn_event_worker(run_id, receiver);
        Ok(run)
    }

    pub fn stop(&self, run_id: &str) -> Result<BenchmarkRun, RunnerError> {
        let (run, handle) = {
            let state = self.state.lock().expect("benchmark manager mutex poisoned");
            let active_run = state.active_runs.get(run_id).ok_or_else(|| {
                RunnerError::new(
                    "run_not_found",
                    format!("benchmark run '{run_id}' is not active"),
                )
            })?;
            (
                state.runs[active_run.run_index].clone(),
                Arc::clone(&active_run.handle),
            )
        };

        handle.stop()?;
        Ok(run)
    }

    pub fn list_runs(&self) -> Vec<NormalizedRunSummary> {
        let state = self.state.lock().expect("benchmark manager mutex poisoned");
        state.runs.iter().map(normalize_run).collect()
    }

    pub fn get_run(&self, run_id: &str) -> Option<BenchmarkRun> {
        let state = self.state.lock().expect("benchmark manager mutex poisoned");
        state.runs.iter().find(|run| run.id == run_id).cloned()
    }

    #[cfg(test)]
    pub fn seed_run_for_test(&self, summary: NormalizedRunSummary) {
        let mut state = self.state.lock().expect("benchmark manager mutex poisoned");
        state.runs.push(BenchmarkRun {
            id: summary.id,
            request: BenchmarkRunRequest {
                runner: summary.runner,
                target_addr: summary.target_addr,
                clients: 1,
                requests: 1,
                data_size: 1,
                pipeline: 1,
            },
            status: summary.status,
            created_at: summary.created_at,
            started_at: None,
            finished_at: summary.finished_at,
            result: None,
            error_message: None,
        });
    }

    fn spawn_event_worker(
        &self,
        run_id: String,
        receiver: mpsc::Receiver<BenchmarkLifecycleEvent>,
    ) {
        let state = Arc::clone(&self.state);
        let sink = Arc::clone(&self.sink);
        let run_events = self.run_events.clone();
        let lifecycle_events = self.lifecycle_events.clone();

        thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                let lifecycle_name = lifecycle_event_name(&event);
                let progress_message = match &event {
                    BenchmarkLifecycleEvent::Progress { message } => Some(message.clone()),
                    _ => None,
                };
                let failure_detail = match &event {
                    BenchmarkLifecycleEvent::Failed { message } => Some(message.clone()),
                    _ => None,
                };
                let mut guard = state.lock().expect("benchmark manager mutex poisoned");
                let Some(run_index) = guard
                    .active_runs
                    .get(&run_id)
                    .map(|active| active.run_index)
                else {
                    break;
                };

                let run = guard
                    .runs
                    .get_mut(run_index)
                    .expect("active run index should exist");
                let before = run.clone();

                match event {
                    BenchmarkLifecycleEvent::Started => {
                        run.status = "running".into();
                        run.started_at.get_or_insert_with(iso_timestamp);
                    }
                    BenchmarkLifecycleEvent::Progress { .. } => {
                        run.status = "running".into();
                    }
                    BenchmarkLifecycleEvent::Completed { result } => {
                        run.status = "completed".into();
                        run.result = Some(result);
                        run.finished_at = Some(iso_timestamp());
                    }
                    BenchmarkLifecycleEvent::Failed { message } => {
                        run.status = "failed".into();
                        run.error_message = Some(message);
                        run.finished_at = Some(iso_timestamp());
                    }
                    BenchmarkLifecycleEvent::Cancelled => {
                        run.status = "cancelled".into();
                        run.finished_at = Some(iso_timestamp());
                    }
                }

                let current = run.clone();
                let changed = current != before;
                let finished = current.finished_at.is_some();
                let lifecycle_changed = progress_message.is_some() || failure_detail.is_some();

                if finished {
                    guard.active_runs.remove(&run_id);
                }

                drop(guard);

                if changed {
                    let _ = run_events.send(current.clone());
                }

                if changed || lifecycle_changed {
                    let _ = lifecycle_events.send(lifecycle_envelope(
                        &current.id,
                        lifecycle_name,
                        progress_message,
                        failure_detail,
                    ));
                }

                if finished {
                    if let Err(error) = sink.persist(&current) {
                        let mut guard = state.lock().expect("benchmark manager mutex poisoned");
                        if let Some(run) = guard.runs.iter_mut().find(|run| run.id == current.id) {
                            run.status = "failed".into();
                            run.error_message = Some(format!("failed to persist run: {error}"));
                            run.finished_at.get_or_insert_with(iso_timestamp);
                            let failed_run = run.clone();
                            drop(guard);
                            let _ = run_events.send(failed_run.clone());
                            let _ = lifecycle_events.send(lifecycle_envelope(
                                &failed_run.id,
                                "failed",
                                None,
                                failed_run.error_message.clone(),
                            ));
                        }
                    }
                    break;
                }
            }
        });
    }
}

fn lifecycle_event_name(event: &BenchmarkLifecycleEvent) -> &'static str {
    match event {
        BenchmarkLifecycleEvent::Started => "started",
        BenchmarkLifecycleEvent::Progress { .. } => "running",
        BenchmarkLifecycleEvent::Completed { .. } => "completed",
        BenchmarkLifecycleEvent::Failed { .. } => "failed",
        BenchmarkLifecycleEvent::Cancelled => "cancelled",
    }
}

fn lifecycle_envelope(
    run_id: &str,
    event: &str,
    message: Option<String>,
    error: Option<String>,
) -> BenchmarkEventEnvelope {
    BenchmarkEventEnvelope {
        channel: BENCHMARK_EVENT_CHANNEL.into(),
        event: event.into(),
        run_id: run_id.into(),
        emitted_at: iso_timestamp(),
        message,
        error,
    }
}

fn normalize_run(run: &BenchmarkRun) -> NormalizedRunSummary {
    NormalizedRunSummary {
        id: run.id.clone(),
        runner: run.request.runner.clone(),
        status: run.status.clone(),
        target_addr: run.request.target_addr.clone(),
        created_at: run.created_at.clone(),
        finished_at: run.finished_at.clone(),
        throughput_ops_per_sec: run
            .result
            .as_ref()
            .map(|result| result.throughput_ops_per_sec),
        p95_latency_ms: run.result.as_ref().map(|result| result.p95_latency_ms),
    }
}

fn next_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();
    format!("run-{millis}")
}

fn iso_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_secs();
    format!("{seconds}Z")
}

#[cfg(test)]
mod tests {
    use super::{BenchmarkManager, BenchmarkRunSink};
    use crate::models::{
        BenchmarkEventEnvelope, BenchmarkResult, BenchmarkRun, BenchmarkRunRequest,
        BENCHMARK_EVENT_CHANNEL,
    };
    use crate::runners::{ActiveBenchmark, BenchmarkLifecycleEvent, BenchmarkRunner, RunnerError};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    struct FakeActiveBenchmark {
        stopped: Arc<AtomicBool>,
        stop_counter: Arc<AtomicBool>,
    }

    impl ActiveBenchmark for FakeActiveBenchmark {
        fn stop(&self) -> Result<(), RunnerError> {
            self.stopped.store(true, Ordering::SeqCst);
            self.stop_counter.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FakeRunner {
        stopped: Arc<AtomicBool>,
        stop_counter: Arc<AtomicBool>,
        scenario: FakeScenario,
    }

    enum FakeScenario {
        Complete,
        Fail,
        WaitForCancel,
        ProgressOnly,
    }

    impl BenchmarkRunner for FakeRunner {
        fn runner_type(&self) -> &'static str {
            "redis-benchmark"
        }

        fn start(
            &self,
            request: BenchmarkRunRequest,
            events: Sender<BenchmarkLifecycleEvent>,
        ) -> Result<Box<dyn ActiveBenchmark>, RunnerError> {
            let stopped = Arc::clone(&self.stopped);
            let scenario = match self.scenario {
                FakeScenario::Complete => FakeScenario::Complete,
                FakeScenario::Fail => FakeScenario::Fail,
                FakeScenario::WaitForCancel => FakeScenario::WaitForCancel,
                FakeScenario::ProgressOnly => FakeScenario::ProgressOnly,
            };

            thread::spawn(move || {
                let _ = events.send(BenchmarkLifecycleEvent::Started);
                let _ = events.send(BenchmarkLifecycleEvent::Progress {
                    message: format!("running {} requests", request.requests),
                });

                match scenario {
                    FakeScenario::Complete => {
                        let _ = events.send(BenchmarkLifecycleEvent::Completed {
                            result: BenchmarkResult {
                                total_requests: request.requests,
                                throughput_ops_per_sec: 2000.0,
                                average_latency_ms: 0.75,
                                p50_latency_ms: 0.50,
                                p95_latency_ms: 1.10,
                                p99_latency_ms: 1.30,
                                duration_ms: 50,
                                dataset_bytes: request.requests * request.data_size as u64,
                            },
                        });
                    }
                    FakeScenario::Fail => {
                        let _ = events.send(BenchmarkLifecycleEvent::Failed {
                            message: "runner failed".into(),
                        });
                    }
                    FakeScenario::WaitForCancel => {
                        for _ in 0..20 {
                            if stopped.load(Ordering::SeqCst) {
                                let _ = events.send(BenchmarkLifecycleEvent::Cancelled);
                                return;
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                    FakeScenario::ProgressOnly => {
                        let _ = events.send(BenchmarkLifecycleEvent::Progress {
                            message: "50% complete".into(),
                        });
                        let _ = events.send(BenchmarkLifecycleEvent::Completed {
                            result: BenchmarkResult {
                                total_requests: request.requests,
                                throughput_ops_per_sec: 1500.0,
                                average_latency_ms: 0.90,
                                p50_latency_ms: 0.60,
                                p95_latency_ms: 1.20,
                                p99_latency_ms: 1.40,
                                duration_ms: 75,
                                dataset_bytes: request.requests * request.data_size as u64,
                            },
                        });
                    }
                }
            });

            Ok(Box::new(FakeActiveBenchmark {
                stopped: Arc::clone(&self.stopped),
                stop_counter: Arc::clone(&self.stop_counter),
            }))
        }
    }

    struct RecordingSink {
        runs: Arc<Mutex<Vec<BenchmarkRun>>>,
        error: Option<String>,
    }

    impl BenchmarkRunSink for RecordingSink {
        fn persist(&self, run: &BenchmarkRun) -> Result<(), String> {
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            self.runs
                .lock()
                .expect("recording sink mutex poisoned")
                .push(run.clone());
            Ok(())
        }
    }

    fn sample_request() -> BenchmarkRunRequest {
        BenchmarkRunRequest {
            runner: "redis-benchmark".into(),
            target_addr: "127.0.0.1:6379".into(),
            clients: 16,
            requests: 200,
            data_size: 64,
            pipeline: 2,
        }
    }

    fn recv_run(receiver: &mut tokio::sync::broadcast::Receiver<BenchmarkRun>) -> BenchmarkRun {
        receiver
            .blocking_recv()
            .expect("benchmark lifecycle event should arrive")
    }

    fn recv_lifecycle(
        receiver: &mut tokio::sync::broadcast::Receiver<BenchmarkEventEnvelope>,
    ) -> BenchmarkEventEnvelope {
        receiver
            .blocking_recv()
            .expect("frontend lifecycle event should arrive")
    }

    fn recv_run_matching(
        receiver: &mut tokio::sync::broadcast::Receiver<BenchmarkRun>,
        status: &str,
    ) -> BenchmarkRun {
        loop {
            let run = recv_run(receiver);
            if run.status == status {
                return run;
            }
        }
    }

    fn recv_lifecycle_matching(
        receiver: &mut tokio::sync::broadcast::Receiver<BenchmarkEventEnvelope>,
        event: &str,
        message: Option<&str>,
    ) -> BenchmarkEventEnvelope {
        loop {
            let envelope = recv_lifecycle(receiver);
            if envelope.event == event
                && message
                    .map(|expected| envelope.message.as_deref() == Some(expected))
                    .unwrap_or(true)
            {
                return envelope;
            }
        }
    }

    #[test]
    fn manager_streams_async_completion_and_persists_finished_run() {
        let persisted = Arc::new(Mutex::new(Vec::new()));
        let manager = BenchmarkManager::with_sink(
            vec![Arc::new(FakeRunner {
                stopped: Arc::new(AtomicBool::new(false)),
                stop_counter: Arc::new(AtomicBool::new(false)),
                scenario: FakeScenario::Complete,
            })],
            Arc::new(RecordingSink {
                runs: Arc::clone(&persisted),
                error: None,
            }),
        );
        let mut events = manager.subscribe();
        let mut lifecycle = manager.subscribe_lifecycle();

        let started = manager.start(sample_request()).expect("run should start");
        let running_event = recv_run(&mut events);
        let started_lifecycle = recv_lifecycle(&mut lifecycle);
        let completed_event = recv_run_matching(&mut events, "completed");
        let completed_lifecycle = recv_lifecycle_matching(&mut lifecycle, "completed", None);

        assert_eq!(started.status, "running");
        assert_eq!(running_event.id, started.id);
        assert_eq!(running_event.status, "running");
        assert_eq!(started_lifecycle.channel, BENCHMARK_EVENT_CHANNEL);
        assert_eq!(started_lifecycle.event, "started");
        assert_eq!(started_lifecycle.run_id, started.id);
        assert!(started_lifecycle.message.is_some());
        assert!(started_lifecycle.error.is_none());
        assert_eq!(completed_event.status, "completed");
        assert_eq!(completed_lifecycle.event, "completed");
        assert_eq!(completed_lifecycle.run_id, started.id);
        assert!(completed_lifecycle.message.is_none());
        assert!(completed_lifecycle.error.is_none());
        assert_eq!(manager.list_runs()[0].status, "completed");
        assert_eq!(
            persisted
                .lock()
                .expect("persisted runs mutex poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn manager_streams_async_failure_and_records_error_message() {
        let manager = BenchmarkManager::new(vec![Arc::new(FakeRunner {
            stopped: Arc::new(AtomicBool::new(false)),
            stop_counter: Arc::new(AtomicBool::new(false)),
            scenario: FakeScenario::Fail,
        })]);
        let mut events = manager.subscribe();
        let mut lifecycle = manager.subscribe_lifecycle();

        let started = manager.start(sample_request()).expect("run should start");
        let _ = recv_run(&mut events);
        let _ = recv_lifecycle(&mut lifecycle);
        let failed = recv_run_matching(&mut events, "failed");
        let failed_lifecycle = recv_lifecycle_matching(&mut lifecycle, "failed", None);

        assert_eq!(started.status, "running");
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error_message, Some("runner failed".into()));
        assert_eq!(failed_lifecycle.channel, BENCHMARK_EVENT_CHANNEL);
        assert_eq!(failed_lifecycle.event, "failed");
        assert_eq!(failed_lifecycle.error, Some("runner failed".into()));
        assert_eq!(manager.list_runs()[0].status, "failed");
    }

    #[test]
    fn manager_stops_active_run_and_broadcasts_cancelled_state() {
        let stopped = Arc::new(AtomicBool::new(false));
        let stop_counter = Arc::new(AtomicBool::new(false));
        let manager = BenchmarkManager::new(vec![Arc::new(FakeRunner {
            stopped: Arc::clone(&stopped),
            stop_counter: Arc::clone(&stop_counter),
            scenario: FakeScenario::WaitForCancel,
        })]);
        let mut events = manager.subscribe();
        let mut lifecycle = manager.subscribe_lifecycle();

        let started = manager.start(sample_request()).expect("run should start");
        let _ = recv_run(&mut events);
        let _ = recv_lifecycle(&mut lifecycle);

        let stop_result = manager.stop(&started.id).expect("run should stop");
        let cancelled = recv_run_matching(&mut events, "cancelled");
        let cancelled_lifecycle = recv_lifecycle_matching(&mut lifecycle, "cancelled", None);

        assert_eq!(stop_result.id, started.id);
        assert!(stopped.load(Ordering::SeqCst));
        assert!(stop_counter.load(Ordering::SeqCst));
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled_lifecycle.channel, BENCHMARK_EVENT_CHANNEL);
        assert_eq!(cancelled_lifecycle.event, "cancelled");
        assert_eq!(cancelled_lifecycle.run_id, started.id);
    }

    #[test]
    fn manager_preserves_progress_detail_in_lifecycle_stream() {
        let manager = BenchmarkManager::new(vec![Arc::new(FakeRunner {
            stopped: Arc::new(AtomicBool::new(false)),
            stop_counter: Arc::new(AtomicBool::new(false)),
            scenario: FakeScenario::ProgressOnly,
        })]);
        let mut lifecycle = manager.subscribe_lifecycle();

        let started = manager.start(sample_request()).expect("run should start");
        let _ = recv_lifecycle(&mut lifecycle);
        let progress = recv_lifecycle_matching(&mut lifecycle, "running", Some("50% complete"));

        assert_eq!(progress.run_id, started.id);
        assert_eq!(progress.event, "running");
        assert_eq!(progress.message, Some("50% complete".into()));
        assert!(progress.error.is_none());
    }

    #[test]
    fn manager_surfaces_persistence_failures_as_failed_lifecycle_and_run_state() {
        let manager = BenchmarkManager::with_sink(
            vec![Arc::new(FakeRunner {
                stopped: Arc::new(AtomicBool::new(false)),
                stop_counter: Arc::new(AtomicBool::new(false)),
                scenario: FakeScenario::Complete,
            })],
            Arc::new(RecordingSink {
                runs: Arc::new(Mutex::new(Vec::new())),
                error: Some("disk full".into()),
            }),
        );
        let mut events = manager.subscribe();
        let mut lifecycle = manager.subscribe_lifecycle();

        let started = manager.start(sample_request()).expect("run should start");
        let _ = recv_run(&mut events);
        let _ = recv_lifecycle(&mut lifecycle);
        let completed = recv_run_matching(&mut events, "completed");
        let completed_lifecycle = recv_lifecycle_matching(&mut lifecycle, "completed", None);
        let failed = recv_run_matching(&mut events, "failed");
        let failed_lifecycle = recv_lifecycle_matching(&mut lifecycle, "failed", None);

        assert_eq!(started.status, "running");
        assert_eq!(completed.status, "completed");
        assert_eq!(completed_lifecycle.event, "completed");
        assert_eq!(failed.status, "failed");
        assert_eq!(
            failed.error_message,
            Some("failed to persist run: disk full".into())
        );
        assert_eq!(failed_lifecycle.event, "failed");
        assert_eq!(
            failed_lifecycle.error,
            Some("failed to persist run: disk full".into())
        );
        assert_eq!(manager.list_runs()[0].status, "failed");
    }
}
