use crate::benchmark_manager::BenchmarkManager;
use crate::info_poller::InfoPoller;
use crate::models::{
    BenchmarkRun, BenchmarkRunRequest, InfoSnapshot, NormalizedRunSummary, ServerStatus,
    StartServerRequest,
};
use crate::run_repository::RunRepository;
use crate::runners::redis_benchmark::RedisBenchmarkRunner;
use crate::server_manager::ServerManager;
use std::sync::Arc;

pub struct AppState {
    benchmark_manager: BenchmarkManager,
    run_repository: Arc<RunRepository>,
    server_manager: ServerManager,
    info_poller: InfoPoller,
}

impl Default for AppState {
    fn default() -> Self {
        let run_repository = Arc::new(default_run_repository());
        Self {
            benchmark_manager: BenchmarkManager::with_sink(
                vec![Arc::new(RedisBenchmarkRunner::new())],
                run_repository.clone(),
            ),
            run_repository,
            server_manager: ServerManager::new(),
            info_poller: InfoPoller::new(),
        }
    }
}

impl AppState {
    pub(crate) fn with_parts(server_manager: ServerManager, info_poller: InfoPoller) -> Self {
        let run_repository = Arc::new(default_run_repository());
        Self {
            benchmark_manager: BenchmarkManager::with_sink(
                vec![Arc::new(RedisBenchmarkRunner::new())],
                run_repository.clone(),
            ),
            run_repository,
            server_manager,
            info_poller,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_components(
        benchmark_manager: BenchmarkManager,
        run_repository: Arc<RunRepository>,
        server_manager: ServerManager,
        info_poller: InfoPoller,
    ) -> Self {
        Self {
            benchmark_manager,
            run_repository,
            server_manager,
            info_poller,
        }
    }

    pub fn list_runs(&self) -> Vec<NormalizedRunSummary> {
        let mut runs = self.run_repository.list_runs().unwrap_or_default();
        for run in self.benchmark_manager.list_runs() {
            if let Some(existing) = runs.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run;
            } else {
                runs.push(run);
            }
        }
        runs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        runs
    }

    pub fn get_run_detail(&self, run_id: &str) -> Result<BenchmarkRun, String> {
        if let Some(run) = self.benchmark_manager.get_run(run_id) {
            return Ok(run);
        }

        self.run_repository
            .get_run(run_id)?
            .ok_or_else(|| format!("benchmark run '{run_id}' was not found"))
    }

    pub fn start_benchmark(&self, request: BenchmarkRunRequest) -> Result<BenchmarkRun, String> {
        self.benchmark_manager
            .start(request)
            .map_err(|error| error.message)
    }

    pub fn stop_benchmark(&self, run_id: &str) -> Result<BenchmarkRun, String> {
        self.benchmark_manager
            .stop(run_id)
            .map_err(|error| error.message)
    }

    pub fn start_server(
        &self,
        request: Option<StartServerRequest>,
    ) -> Result<ServerStatus, String> {
        self.server_manager.start(request)
    }

    pub fn stop_server(&self) -> Result<ServerStatus, String> {
        let status = self.server_manager.stop()?;
        self.info_poller.clear();
        Ok(status)
    }

    pub fn server_status(&self) -> ServerStatus {
        let mut status = self.server_manager.status();
        if let Some(error) = self.info_poller.take_error() {
            status.last_error = Some(error);
        }
        status
    }

    pub fn info_snapshot(&self) -> Option<InfoSnapshot> {
        let status = self.server_manager.status();
        if status.state == "running" {
            match self.info_poller.poll(&status.address) {
                Ok(snapshot) => Some(snapshot),
                Err(_) => None,
            }
        } else {
            self.info_poller.snapshot()
        }
    }
}

fn default_run_repository() -> RunRepository {
    let storage_root = preferred_state_root().join("hybird-kv-gui").join("runs");
    RunRepository::new(storage_root).expect("run repository should initialize")
}

fn preferred_state_root() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed)
                .join(".local")
                .join("state");
        }
    }

    std::env::temp_dir()
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::benchmark_manager::BenchmarkManager;
    use crate::info_poller::{InfoClient, InfoClientFactory, InfoPoller};
    use crate::models::{BenchmarkRunRequest, NormalizedRunSummary};
    use crate::run_repository::RunRepository;
    use crate::runners::redis_benchmark::RedisBenchmarkRunner;
    use crate::server_manager::{LaunchSpec, ManagedChild, ProcessLauncher, ServerManager};
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone)]
    struct FakeLauncher {
        starts: Arc<Mutex<VecDeque<FakeChild>>>,
    }

    impl FakeLauncher {
        fn running(pid: u32) -> Self {
            Self {
                starts: Arc::new(Mutex::new(vec![FakeChild::running(pid)].into())),
            }
        }
    }

    impl ProcessLauncher for FakeLauncher {
        fn spawn(&self, _spec: &LaunchSpec) -> Result<Box<dyn ManagedChild>, String> {
            let child = self
                .starts
                .lock()
                .expect("starts mutex poisoned")
                .pop_front()
                .expect("fake child should exist");
            Ok(Box::new(child))
        }
    }

    struct FakeChild {
        pid: u32,
        waits: VecDeque<Result<Option<i32>, std::io::Error>>,
    }

    impl FakeChild {
        fn running(pid: u32) -> Self {
            Self {
                pid,
                waits: vec![Ok(None), Ok(None), Ok(None)].into(),
            }
        }
    }

    impl ManagedChild for FakeChild {
        fn id(&self) -> u32 {
            self.pid
        }

        fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
            self.waits.pop_front().unwrap_or(Ok(None))
        }

        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn wait(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FailingClientFactory;

    impl InfoClientFactory for FailingClientFactory {
        fn connect(&self, _address: &str) -> Result<Box<dyn InfoClient>, String> {
            Ok(Box::new(FailingClient))
        }
    }

    struct FailingClient;

    impl InfoClient for FailingClient {
        fn info(&self) -> Result<String, String> {
            Err("poll failed".into())
        }
    }

    fn temp_storage_dir(test_name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hkv-app-state-{test_name}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn app_state_provides_default_contract_handles() {
        let state = AppState::default();

        assert!(state.list_runs().is_empty());
        assert_eq!(state.server_status().state, "stopped");
        assert_eq!(state.server_status().address, "127.0.0.1:6380");
        assert!(state.info_snapshot().is_none());
    }

    #[test]
    fn app_state_surfaces_info_poll_failures_through_server_status() {
        let manager = ServerManager::with_launcher(Box::new(FakeLauncher::running(4242)));
        manager
            .start(None)
            .expect("server manager should report running state");
        let poller = InfoPoller::with_client_factory(Box::new(FailingClientFactory));
        let state = AppState::with_parts(manager, poller);

        assert!(state.info_snapshot().is_none());
        assert_eq!(state.server_status().last_error, Some("poll failed".into()));
    }

    #[test]
    fn app_state_merges_persisted_and_active_run_summaries() {
        let storage_dir = temp_storage_dir("merge-runs");
        let repository = Arc::new(
            RunRepository::new(storage_dir.clone()).expect("repository should initialize"),
        );
        let benchmark_manager = BenchmarkManager::new(vec![Arc::new(RedisBenchmarkRunner::new())]);
        repository
            .store_runs_for_test(&[crate::models::BenchmarkRun {
                id: "persisted-001".into(),
                request: BenchmarkRunRequest {
                    runner: "redis-benchmark".into(),
                    target_addr: "127.0.0.1:6379".into(),
                    clients: 8,
                    requests: 1000,
                    data_size: 64,
                    pipeline: 1,
                },
                status: "completed".into(),
                created_at: "2026-03-23T12:00:00Z".into(),
                started_at: Some("2026-03-23T12:00:01Z".into()),
                finished_at: Some("2026-03-23T12:00:02Z".into()),
                result: None,
                error_message: None,
            }])
            .expect("seed runs should persist");

        let state = AppState::with_components(
            benchmark_manager,
            Arc::clone(&repository),
            ServerManager::with_launcher(Box::new(FakeLauncher::running(1))),
            InfoPoller::new(),
        );

        state
            .benchmark_manager
            .seed_run_for_test(NormalizedRunSummary {
                id: "active-001".into(),
                runner: "redis-benchmark".into(),
                status: "running".into(),
                target_addr: "127.0.0.1:6379".into(),
                created_at: "2026-03-23T12:00:03Z".into(),
                finished_at: None,
                throughput_ops_per_sec: None,
                p95_latency_ms: None,
            });

        let merged = state.list_runs();

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|run| run.id == "persisted-001"));
        assert!(merged.iter().any(|run| run.id == "active-001"));

        fs::remove_dir_all(storage_dir).expect("temp dir should be removed");
    }
}
