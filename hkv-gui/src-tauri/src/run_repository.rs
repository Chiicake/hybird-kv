use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::benchmark_manager::BenchmarkRunSink;
use crate::models::{BenchmarkRun, NormalizedRunSummary};

const RUN_STORE_FILE: &str = "runs.json";

pub struct RunRepository {
    store_path: PathBuf,
    write_lock: Mutex<()>,
}

impl RunRepository {
    pub fn new(storage_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&storage_dir).map_err(|error| error.to_string())?;
        let store_path = storage_dir.join(RUN_STORE_FILE);
        if !store_path.exists() {
            fs::write(&store_path, "[]").map_err(|error| error.to_string())?;
        }

        Ok(Self {
            store_path,
            write_lock: Mutex::new(()),
        })
    }

    pub fn list_runs(&self) -> Result<Vec<NormalizedRunSummary>, String> {
        let mut runs = self.load_runs()?;
        runs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(runs.iter().map(normalize_run).collect())
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<BenchmarkRun>, String> {
        let runs = self.load_runs()?;
        Ok(runs.into_iter().find(|run| run.id == run_id))
    }

    fn load_runs(&self) -> Result<Vec<BenchmarkRun>, String> {
        let contents = fs::read_to_string(&self.store_path).map_err(|error| error.to_string())?;
        match serde_json::from_str(&contents) {
            Ok(runs) => Ok(runs),
            Err(_) => {
                self.backup_corrupted_store(&contents)?;
                self.store_runs(&[])?;
                Ok(Vec::new())
            }
        }
    }

    fn store_runs(&self, runs: &[BenchmarkRun]) -> Result<(), String> {
        let payload = serde_json::to_string_pretty(runs).map_err(|error| error.to_string())?;
        fs::write(&self.store_path, payload).map_err(|error| error.to_string())
    }

    fn backup_corrupted_store(&self, contents: &str) -> Result<(), String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let backup_path = self
            .store_path
            .with_file_name(format!("runs.corrupt-{unique}.json"));
        fs::write(backup_path, contents).map_err(|error| error.to_string())
    }
}

impl BenchmarkRunSink for RunRepository {
    fn persist(&self, run: &BenchmarkRun) -> Result<(), String> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| "run repository mutex poisoned".to_string())?;
        let mut runs = self.load_runs()?;

        if let Some(existing) = runs.iter_mut().find(|existing| existing.id == run.id) {
            *existing = run.clone();
        } else {
            runs.push(run.clone());
        }

        self.store_runs(&runs)
    }
}

#[cfg(test)]
impl RunRepository {
    pub fn store_runs_for_test(&self, runs: &[BenchmarkRun]) -> Result<(), String> {
        self.store_runs(runs)
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

#[cfg(test)]
mod tests {
    use super::{RunRepository, RUN_STORE_FILE};
    use crate::benchmark_manager::BenchmarkRunSink;
    use crate::models::{BenchmarkResult, BenchmarkRun, BenchmarkRunRequest};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_storage_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hkv-run-repo-{test_name}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn sample_run(id: &str, status: &str, throughput: f64, p95_latency_ms: f64) -> BenchmarkRun {
        BenchmarkRun {
            id: id.into(),
            request: BenchmarkRunRequest {
                runner: "redis-benchmark".into(),
                target_addr: "127.0.0.1:6379".into(),
                clients: 32,
                requests: 100_000,
                data_size: 128,
                pipeline: 4,
            },
            status: status.into(),
            created_at: format!("2026-03-23T12:00:0{}Z", &id[id.len() - 1..]),
            started_at: Some(format!("2026-03-23T12:00:0{}Z", &id[id.len() - 1..])),
            finished_at: Some(format!("2026-03-23T12:00:1{}Z", &id[id.len() - 1..])),
            result: Some(BenchmarkResult {
                total_requests: 100_000,
                throughput_ops_per_sec: throughput,
                average_latency_ms: 1.2,
                p50_latency_ms: 0.9,
                p95_latency_ms,
                p99_latency_ms: p95_latency_ms + 0.8,
                duration_ms: 5_000,
                dataset_bytes: 12_800_000,
            }),
            error_message: None,
        }
    }

    #[test]
    fn persists_runs_and_returns_summaries_in_reverse_chronological_order() {
        let storage_dir = temp_storage_dir("list-order");
        let repository =
            RunRepository::new(storage_dir.clone()).expect("repository should initialize");
        let older = sample_run("run-001", "completed", 101_000.0, 2.4);
        let newer = sample_run("run-002", "completed", 125_000.0, 1.8);

        repository
            .persist(&older)
            .expect("older run should persist");
        repository
            .persist(&newer)
            .expect("newer run should persist");

        let runs = repository.list_runs().expect("runs should load");

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].id, "run-002");
        assert_eq!(runs[0].throughput_ops_per_sec, Some(125_000.0));
        assert_eq!(runs[1].id, "run-001");
        assert_eq!(runs[1].p95_latency_ms, Some(2.4));

        fs::remove_dir_all(storage_dir).expect("temp dir should be removed");
    }

    #[test]
    fn empty_repository_returns_no_runs_and_no_detail() {
        let storage_dir = temp_storage_dir("empty-state");
        let repository =
            RunRepository::new(storage_dir.clone()).expect("repository should initialize");

        let runs = repository.list_runs().expect("empty runs should load");
        let detail = repository
            .get_run("missing")
            .expect("lookup should succeed");

        assert!(runs.is_empty());
        assert!(detail.is_none());

        fs::remove_dir_all(storage_dir).expect("temp dir should be removed");
    }

    #[test]
    fn updates_existing_run_records_and_loads_detail_by_id() {
        let storage_dir = temp_storage_dir("detail-update");
        let repository =
            RunRepository::new(storage_dir.clone()).expect("repository should initialize");
        let mut run = sample_run("run-101", "running", 99_000.0, 2.1);
        run.finished_at = None;
        run.result = None;

        repository
            .persist(&run)
            .expect("initial run should persist");

        run.status = "completed".into();
        run.finished_at = Some("2026-03-23T12:10:05Z".into());
        run.result = Some(BenchmarkResult {
            total_requests: 100_000,
            throughput_ops_per_sec: 144_000.0,
            average_latency_ms: 1.1,
            p50_latency_ms: 0.8,
            p95_latency_ms: 1.7,
            p99_latency_ms: 2.3,
            duration_ms: 4_200,
            dataset_bytes: 12_800_000,
        });

        repository
            .persist(&run)
            .expect("updated run should persist");

        let detail = repository
            .get_run("run-101")
            .expect("lookup should succeed")
            .expect("run should exist");

        assert_eq!(detail.status, "completed");
        assert_eq!(detail.finished_at.as_deref(), Some("2026-03-23T12:10:05Z"));
        assert_eq!(
            detail
                .result
                .as_ref()
                .map(|result| result.throughput_ops_per_sec),
            Some(144_000.0)
        );

        fs::remove_dir_all(storage_dir).expect("temp dir should be removed");
    }

    #[test]
    fn corrupted_repository_data_recovers_to_empty_state() {
        let storage_dir = temp_storage_dir("corrupt-state");
        let repository =
            RunRepository::new(storage_dir.clone()).expect("repository should initialize");
        let store_path = storage_dir.join(RUN_STORE_FILE);

        fs::write(&store_path, "{ definitely not valid json").expect("corrupt data should write");

        let runs = repository
            .list_runs()
            .expect("corrupt repository should recover");
        let repaired = fs::read_to_string(&store_path).expect("repaired store should read");
        let backup_count = fs::read_dir(&storage_dir)
            .expect("storage dir should read")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("runs.corrupt-")
            })
            .count();

        assert!(runs.is_empty());
        assert_eq!(repaired.trim(), "[]");
        assert_eq!(backup_count, 1);

        fs::remove_dir_all(storage_dir).expect("temp dir should be removed");
    }
}
