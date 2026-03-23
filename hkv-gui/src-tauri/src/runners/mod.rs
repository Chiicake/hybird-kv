use std::path::Path;
use std::sync::Arc;

use crate::models::{BenchmarkResult, BenchmarkRunRequest};

pub mod redis_benchmark;

#[derive(Debug, Clone, PartialEq)]
pub enum BenchmarkLifecycleEvent {
    Started,
    Progress { message: String },
    Completed { result: BenchmarkResult },
    Failed { message: String },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerError {
    pub code: String,
    pub message: String,
}

impl RunnerError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub trait ActiveBenchmark: Send + Sync {
    fn stop(&self) -> Result<(), RunnerError>;
}

pub trait BenchmarkRunner: Send + Sync {
    fn runner_type(&self) -> &'static str;
    fn start(
        &self,
        request: BenchmarkRunRequest,
        events: std::sync::mpsc::Sender<BenchmarkLifecycleEvent>,
    ) -> Result<Box<dyn ActiveBenchmark>, RunnerError>;
}

pub fn select_runner(
    runners: &[Arc<dyn BenchmarkRunner>],
    runner_type: &str,
) -> Result<Arc<dyn BenchmarkRunner>, RunnerError> {
    let normalized = normalize_runner_type(runner_type);

    runners
        .iter()
        .find(|runner| runner.runner_type() == normalized)
        .cloned()
        .ok_or_else(|| {
            RunnerError::new(
                "unsupported_runner",
                format!("runner '{runner_type}' is not supported"),
            )
        })
}

fn normalize_runner_type(runner_type: &str) -> &str {
    let file_name = Path::new(runner_type)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(runner_type);

    if file_name.eq_ignore_ascii_case("redis-benchmark")
        || file_name.eq_ignore_ascii_case("redis-benchmark.exe")
    {
        "redis-benchmark"
    } else {
        runner_type
    }
}

#[cfg(test)]
mod tests {
    use super::{select_runner, BenchmarkLifecycleEvent, BenchmarkRunner, RunnerError};
    use crate::models::{BenchmarkResult, BenchmarkRunRequest};
    use std::sync::Arc;

    struct FakeRunner {
        runner_type: &'static str,
    }

    impl BenchmarkRunner for FakeRunner {
        fn runner_type(&self) -> &'static str {
            self.runner_type
        }

        fn start(
            &self,
            _request: BenchmarkRunRequest,
            _events: std::sync::mpsc::Sender<BenchmarkLifecycleEvent>,
        ) -> Result<Box<dyn super::ActiveBenchmark>, RunnerError> {
            Err(RunnerError::new("not_used", "not used in selection tests"))
        }
    }

    #[test]
    fn selects_runner_by_runner_type() {
        let runners: Vec<Arc<dyn BenchmarkRunner>> = vec![
            Arc::new(FakeRunner {
                runner_type: "redis-benchmark",
            }),
            Arc::new(FakeRunner {
                runner_type: "hkv-bench",
            }),
        ];

        let selected = select_runner(&runners, "redis-benchmark").expect("runner should resolve");

        assert_eq!(selected.runner_type(), "redis-benchmark");
    }

    #[test]
    fn reports_unsupported_runner_clearly() {
        let runners: Vec<Arc<dyn BenchmarkRunner>> = vec![Arc::new(FakeRunner {
            runner_type: "redis-benchmark",
        })];

        let result = select_runner(&runners, "unknown-runner");
        assert!(result.is_err());
        let error = result.err().expect("runner lookup should fail");

        assert_eq!(error.code, "unsupported_runner");
        assert_eq!(error.message, "runner 'unknown-runner' is not supported");
    }

    #[test]
    fn resolves_redis_runner_when_request_uses_an_explicit_binary_path() {
        let runners: Vec<Arc<dyn BenchmarkRunner>> = vec![Arc::new(FakeRunner {
            runner_type: "redis-benchmark",
        })];

        let selected = select_runner(&runners, "/usr/local/bin/redis-benchmark")
            .expect("redis-benchmark path should resolve to redis adapter");

        assert_eq!(selected.runner_type(), "redis-benchmark");
    }

    #[test]
    fn lifecycle_events_cover_started_running_completed_and_failed_states() {
        let completed = BenchmarkLifecycleEvent::Completed {
            result: BenchmarkResult {
                total_requests: 100,
                throughput_ops_per_sec: 999.0,
                average_latency_ms: 1.2,
                p50_latency_ms: 1.0,
                p95_latency_ms: 2.0,
                p99_latency_ms: 3.0,
                duration_ms: 100,
                dataset_bytes: 1024,
            },
        };

        assert!(matches!(
            BenchmarkLifecycleEvent::Started,
            BenchmarkLifecycleEvent::Started
        ));
        assert!(matches!(
            BenchmarkLifecycleEvent::Progress {
                message: "running".into()
            },
            BenchmarkLifecycleEvent::Progress { .. }
        ));
        assert!(matches!(
            completed,
            BenchmarkLifecycleEvent::Completed { .. }
        ));
        assert!(matches!(
            BenchmarkLifecycleEvent::Failed {
                message: "boom".into()
            },
            BenchmarkLifecycleEvent::Failed { .. }
        ));
    }
}
