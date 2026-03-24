use crate::models::{BenchmarkResult, BenchmarkRunRequest};
use crate::runners::{ActiveBenchmark, BenchmarkLifecycleEvent, BenchmarkRunner, RunnerError};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc::Sender, Arc, Mutex};
use std::thread;

const REDIS_BENCHMARK_BINARY: &str = "redis-benchmark";

pub trait BenchmarkProcess: Send {
    fn kill(&mut self) -> std::io::Result<()>;
    fn wait(&mut self) -> std::io::Result<i32>;
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>>;
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>>;
}

pub trait BenchmarkProcessSpawner: Send + Sync {
    fn spawn(
        &self,
        program: &str,
        args: &[String],
    ) -> Result<Box<dyn BenchmarkProcess>, RunnerError>;
}

pub struct SystemBenchmarkProcessSpawner;

pub struct RedisBenchmarkRunner {
    spawner: Arc<dyn BenchmarkProcessSpawner>,
}

struct SystemBenchmarkProcess {
    child: Child,
}

struct RedisBenchmarkHandle {
    process: Arc<Mutex<Option<Box<dyn BenchmarkProcess>>>>,
    cancelled: Arc<AtomicBool>,
}

impl BenchmarkProcess for SystemBenchmarkProcess {
    fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }

    fn wait(&mut self) -> std::io::Result<i32> {
        Ok(self.child.wait()?.code().unwrap_or(-1))
    }

    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.child
            .stdout
            .take()
            .map(|stdout| Box::new(stdout) as Box<dyn Read + Send>)
    }

    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.child
            .stderr
            .take()
            .map(|stderr| Box::new(stderr) as Box<dyn Read + Send>)
    }
}

impl BenchmarkProcessSpawner for SystemBenchmarkProcessSpawner {
    fn spawn(
        &self,
        program: &str,
        args: &[String],
    ) -> Result<Box<dyn BenchmarkProcess>, RunnerError> {
        let mut command = Command::new(program);
        command.args(args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let child = command.spawn().map_err(|err| {
            let code = if err.kind() == std::io::ErrorKind::NotFound {
                "binary_missing"
            } else {
                "spawn_failed"
            };
            RunnerError::new(code, format!("failed to start redis-benchmark: {err}"))
        })?;

        Ok(Box::new(SystemBenchmarkProcess { child }))
    }
}

impl RedisBenchmarkRunner {
    pub fn new() -> Self {
        Self::with_spawner(Arc::new(SystemBenchmarkProcessSpawner))
    }

    pub fn with_spawner(spawner: Arc<dyn BenchmarkProcessSpawner>) -> Self {
        Self { spawner }
    }

    pub fn build_command(&self, request: &BenchmarkRunRequest) -> Result<Vec<String>, RunnerError> {
        validate_request(request)?;
        let (host, port) = split_target_addr(&request.target_addr)?;

        Ok(vec![
            "-h".into(),
            host,
            "-p".into(),
            port.to_string(),
            "-c".into(),
            request.clients.to_string(),
            "-n".into(),
            request.requests.to_string(),
            "-d".into(),
            request.data_size.to_string(),
            "-P".into(),
            request.pipeline.to_string(),
            "--csv".into(),
        ])
    }

    pub fn parse_output(&self, stdout: &str, stderr: &str) -> Result<BenchmarkResult, RunnerError> {
        let mut throughput_ops_per_sec = None;
        let mut total_requests = None;
        let mut dataset_bytes = None;
        let mut duration_ms = None;
        let mut average_latency_ms = None;
        let mut p50_latency_ms = None;
        let mut p95_latency_ms = None;
        let mut p99_latency_ms = None;

        for raw_line in stdout.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('"') && line.contains("PING_INLINE") {
                let segments: Vec<&str> = line.split(',').collect();
                if segments.len() >= 2 {
                    throughput_ops_per_sec = Some(parse_csv_number(segments[1])?);
                }
            }

            if line.contains("latency summary") {
                p50_latency_ms = Some(parse_key_value_number(line, "p50")?);
                p95_latency_ms = Some(parse_key_value_number(line, "p95")?);
                p99_latency_ms = Some(parse_key_value_number(line, "p99")?);
            }

            if line.contains("requests summary") {
                total_requests = Some(parse_key_value_u64(line, "requests")?);
                dataset_bytes = Some(parse_key_value_u64(line, "bytes")?);
                duration_ms = Some(parse_key_value_u64(line, "duration_ms")?);
                average_latency_ms = Some(parse_key_value_number(line, "avg_ms")?);
            }
        }

        match (
            total_requests,
            throughput_ops_per_sec,
            average_latency_ms,
            p50_latency_ms,
            p95_latency_ms,
            p99_latency_ms,
            duration_ms,
            dataset_bytes,
        ) {
            (
                Some(total_requests),
                Some(throughput_ops_per_sec),
                Some(average_latency_ms),
                Some(p50_latency_ms),
                Some(p95_latency_ms),
                Some(p99_latency_ms),
                Some(duration_ms),
                Some(dataset_bytes),
            ) => Ok(BenchmarkResult {
                total_requests,
                throughput_ops_per_sec,
                average_latency_ms,
                p50_latency_ms,
                p95_latency_ms,
                p99_latency_ms,
                duration_ms,
                dataset_bytes,
            }),
            _ => Err(RunnerError::new(
                "parse_failed",
                format!("failed to parse redis-benchmark output: {stderr}"),
            )),
        }
    }
}

impl ActiveBenchmark for RedisBenchmarkHandle {
    fn stop(&self) -> Result<(), RunnerError> {
        self.cancelled.store(true, Ordering::SeqCst);

        let mut process = self
            .process
            .lock()
            .expect("redis benchmark process mutex poisoned");
        let process = process.as_mut().ok_or_else(|| {
            RunnerError::new(
                "run_not_active",
                "redis-benchmark process is no longer active",
            )
        })?;

        process.kill().map_err(|err| {
            RunnerError::new(
                "stop_failed",
                format!("failed to stop redis-benchmark: {err}"),
            )
        })
    }
}

impl BenchmarkRunner for RedisBenchmarkRunner {
    fn runner_type(&self) -> &'static str {
        "redis-benchmark"
    }

    fn start(
        &self,
        request: BenchmarkRunRequest,
        events: Sender<BenchmarkLifecycleEvent>,
    ) -> Result<Box<dyn ActiveBenchmark>, RunnerError> {
        validate_request(&request)?;
        let args = self.build_command(&request)?;
        let mut process = self.spawner.spawn(REDIS_BENCHMARK_BINARY, &args)?;
        let stdout = process.take_stdout().ok_or_else(|| {
            RunnerError::new("spawn_failed", "redis-benchmark stdout was not captured")
        })?;
        let stderr = process.take_stderr().ok_or_else(|| {
            RunnerError::new("spawn_failed", "redis-benchmark stderr was not captured")
        })?;

        let process = Arc::new(Mutex::new(Some(process)));
        let cancelled = Arc::new(AtomicBool::new(false));

        let _ = events.send(BenchmarkLifecycleEvent::Started);
        let _ = events.send(BenchmarkLifecycleEvent::Progress {
            message: format!(
                "started redis-benchmark against {} with {} clients",
                request.target_addr, request.clients
            ),
        });

        let observer_process = Arc::clone(&process);
        let observer_cancelled = Arc::clone(&cancelled);
        let observer_events = events.clone();
        let parser = RedisBenchmarkRunner::new();

        thread::spawn(move || {
            let stdout_events = observer_events.clone();
            let stderr_events = observer_events.clone();
            let stdout_reader = stdout;
            let stderr_reader = stderr;

            let stdout_thread = thread::spawn(move || read_stream(stdout_reader, stdout_events));
            let stderr_thread = thread::spawn(move || read_stream(stderr_reader, stderr_events));

            let exit_code = {
                let mut guard = observer_process
                    .lock()
                    .expect("redis benchmark process mutex poisoned");
                let mut process = match guard.take() {
                    Some(process) => process,
                    None => return,
                };

                process.wait()
            };

            let stdout = stdout_thread.join().unwrap_or_else(|_| String::new());
            let stderr = stderr_thread.join().unwrap_or_else(|_| String::new());

            match exit_code {
                Ok(code) if observer_cancelled.load(Ordering::SeqCst) => {
                    let _ = observer_events.send(BenchmarkLifecycleEvent::Cancelled);
                    let _ = observer_events.send(BenchmarkLifecycleEvent::Progress {
                        message: "redis-benchmark cancelled".into(),
                    });
                    let _ = code;
                }
                Ok(0) => match parser.parse_output(&stdout, &stderr) {
                    Ok(result) => {
                        let _ = observer_events.send(BenchmarkLifecycleEvent::Completed { result });
                    }
                    Err(error) => {
                        let _ = observer_events.send(BenchmarkLifecycleEvent::Failed {
                            message: error.message,
                        });
                    }
                },
                Ok(code) => {
                    let message = if stderr.trim().is_empty() {
                        format!("redis-benchmark exited early with code {code}")
                    } else {
                        format!("redis-benchmark exited early with code {code}: {stderr}")
                    };
                    let _ = observer_events.send(BenchmarkLifecycleEvent::Failed { message });
                }
                Err(err) => {
                    let _ = observer_events.send(BenchmarkLifecycleEvent::Failed {
                        message: format!("failed to wait for redis-benchmark: {err}"),
                    });
                }
            }
        });

        Ok(Box::new(RedisBenchmarkHandle { process, cancelled }))
    }
}

fn read_stream(
    mut reader: Box<dyn Read + Send>,
    events: Sender<BenchmarkLifecycleEvent>,
) -> String {
    let mut buffer = String::new();
    let _ = reader.read_to_string(&mut buffer);

    let mut lines = BufReader::new(buffer.as_bytes());
    let mut line = String::new();
    loop {
        line.clear();
        match lines.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = events.send(BenchmarkLifecycleEvent::Progress {
                        message: trimmed.to_string(),
                    });
                }
            }
            Err(_) => break,
        }
    }

    buffer
}

fn validate_request(request: &BenchmarkRunRequest) -> Result<(), RunnerError> {
    if request.clients == 0
        || request.requests == 0
        || request.data_size == 0
        || request.pipeline == 0
    {
        return Err(RunnerError::new(
            "invalid_request",
            "clients, requests, data_size, and pipeline must be greater than zero",
        ));
    }

    let _ = split_target_addr(&request.target_addr)?;
    Ok(())
}

fn split_target_addr(target_addr: &str) -> Result<(String, u16), RunnerError> {
    let (host, port) = target_addr.split_once(':').ok_or_else(|| {
        RunnerError::new(
            "invalid_request",
            format!("target address '{target_addr}' must use host:port format"),
        )
    })?;

    if host.is_empty() {
        return Err(RunnerError::new(
            "invalid_request",
            "target host must not be empty",
        ));
    }

    let port = port.parse::<u16>().map_err(|_| {
        RunnerError::new(
            "invalid_request",
            format!("target address '{target_addr}' has an invalid port"),
        )
    })?;

    Ok((host.to_string(), port))
}

fn parse_csv_number(segment: &str) -> Result<f64, RunnerError> {
    segment.trim_matches('"').parse::<f64>().map_err(|_| {
        RunnerError::new(
            "parse_failed",
            format!("failed to parse redis-benchmark throughput from '{segment}'"),
        )
    })
}

fn parse_key_value_number(line: &str, key: &str) -> Result<f64, RunnerError> {
    let value = parse_key_value_segment(line, key)?;
    value.parse::<f64>().map_err(|_| {
        RunnerError::new(
            "parse_failed",
            format!("failed to parse {key} from '{line}'"),
        )
    })
}

fn parse_key_value_u64(line: &str, key: &str) -> Result<u64, RunnerError> {
    let value = parse_key_value_segment(line, key)?;
    value.parse::<u64>().map_err(|_| {
        RunnerError::new(
            "parse_failed",
            format!("failed to parse {key} from '{line}'"),
        )
    })
}

fn parse_key_value_segment(line: &str, key: &str) -> Result<String, RunnerError> {
    line.replace([',', '"'], " ")
        .split_whitespace()
        .find_map(|segment| segment.strip_prefix(&format!("{key}=")).map(str::to_owned))
        .ok_or_else(|| RunnerError::new("parse_failed", format!("missing {key} in '{line}'")))
}

#[cfg(test)]
mod tests {
    use super::{BenchmarkProcess, BenchmarkProcessSpawner, RedisBenchmarkRunner};
    use crate::models::BenchmarkRunRequest;
    use crate::runners::{BenchmarkLifecycleEvent, BenchmarkRunner};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};

    struct FakeProcess {
        exit_code: i32,
        stdout: Option<Box<dyn std::io::Read + Send>>,
        stderr: Option<Box<dyn std::io::Read + Send>>,
        killed: Arc<AtomicBool>,
    }

    impl BenchmarkProcess for FakeProcess {
        fn kill(&mut self) -> std::io::Result<()> {
            self.killed.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn wait(&mut self) -> std::io::Result<i32> {
            Ok(self.exit_code)
        }

        fn take_stdout(&mut self) -> Option<Box<dyn std::io::Read + Send>> {
            self.stdout.take()
        }

        fn take_stderr(&mut self) -> Option<Box<dyn std::io::Read + Send>> {
            self.stderr.take()
        }
    }

    struct FakeSpawner {
        process: Mutex<Option<FakeProcess>>,
        error: Option<(String, String)>,
        spawn_calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl FakeSpawner {
        fn successful(process: FakeProcess) -> Self {
            Self {
                process: Mutex::new(Some(process)),
                error: None,
                spawn_calls: Mutex::new(Vec::new()),
            }
        }

        fn failing(code: &str, message: &str) -> Self {
            Self {
                process: Mutex::new(None),
                error: Some((code.into(), message.into())),
                spawn_calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl BenchmarkProcessSpawner for FakeSpawner {
        fn spawn(
            &self,
            program: &str,
            args: &[String],
        ) -> Result<Box<dyn BenchmarkProcess>, crate::runners::RunnerError> {
            self.spawn_calls
                .lock()
                .expect("spawn calls mutex poisoned")
                .push((program.into(), args.to_vec()));

            if let Some((code, message)) = &self.error {
                return Err(crate::runners::RunnerError::new(
                    code.clone(),
                    message.clone(),
                ));
            }

            Ok(Box::new(
                self.process
                    .lock()
                    .expect("process mutex poisoned")
                    .take()
                    .expect("fake process should exist"),
            ))
        }
    }

    fn sample_request() -> BenchmarkRunRequest {
        BenchmarkRunRequest {
            runner: "redis-benchmark".into(),
            target_addr: "127.0.0.1:6379".into(),
            clients: 50,
            requests: 100000,
            data_size: 128,
            pipeline: 4,
        }
    }

    fn completed_stdout() -> &'static str {
        concat!(
            "\"PING_INLINE\",\"104166.66\"\n",
            "\"latency summary\",\"min=0.120 p50=0.440 p95=0.880 p99=1.020 max=2.200\"\n",
            "\"requests summary\",\"requests=100000 bytes=12800000 duration_ms=960 avg_ms=0.520\""
        )
    }

    fn recv_event(receiver: &mpsc::Receiver<BenchmarkLifecycleEvent>) -> BenchmarkLifecycleEvent {
        receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("event should arrive")
    }

    #[test]
    fn builds_redis_benchmark_command_from_request() {
        let runner = RedisBenchmarkRunner::new();

        let command = runner
            .build_command(&sample_request())
            .expect("command should build");

        assert_eq!(
            command,
            vec![
                "-h",
                "127.0.0.1",
                "-p",
                "6379",
                "-c",
                "50",
                "-n",
                "100000",
                "-d",
                "128",
                "-P",
                "4",
                "--csv",
            ]
        );
    }

    #[test]
    fn build_command_does_not_depend_on_runner_type_field() {
        let runner = RedisBenchmarkRunner::new();
        let mut request = sample_request();
        request.runner = "future-runner-name".into();

        let command = runner
            .build_command(&request)
            .expect("command should only validate executable inputs");

        assert_eq!(command[0], "-h");
        assert_eq!(command[1], "127.0.0.1");
    }

    #[test]
    fn parses_final_metrics_into_normalized_result() {
        let runner = RedisBenchmarkRunner::new();
        let result = runner
            .parse_output(completed_stdout(), "")
            .expect("output should parse");

        assert_eq!(result.total_requests, 100000);
        assert_eq!(result.throughput_ops_per_sec, 104166.66);
        assert_eq!(result.average_latency_ms, 0.520);
        assert_eq!(result.p50_latency_ms, 0.440);
        assert_eq!(result.p95_latency_ms, 0.880);
        assert_eq!(result.p99_latency_ms, 1.020);
        assert_eq!(result.duration_ms, 960);
        assert_eq!(result.dataset_bytes, 12800000);
    }

    #[test]
    fn start_streams_stdout_and_stderr_lines_as_progress_events() {
        let spawner = Arc::new(FakeSpawner::successful(FakeProcess {
            exit_code: 0,
            stdout: Some(Box::new(Cursor::new(
                b"PING_INLINE,1000.0\nrequests summary requests=100 bytes=12800 duration_ms=10 avg_ms=0.2\nlatency summary p50=0.1 p95=0.3 p99=0.5\n"
                    .to_vec(),
            ))),
            stderr: Some(Box::new(Cursor::new(b"WARNING: warmup\n".to_vec()))),
            killed: Arc::new(AtomicBool::new(false)),
        }));
        let runner = RedisBenchmarkRunner::with_spawner(spawner);
        let (sender, receiver) = mpsc::channel();

        let _handle = runner
            .start(sample_request(), sender)
            .expect("runner should start");

        let events: Vec<BenchmarkLifecycleEvent> = receiver.iter().take(7).collect();

        assert!(events.iter().any(|event| matches!(
            event,
            BenchmarkLifecycleEvent::Progress { message } if message.contains("PING_INLINE,1000.0")
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            BenchmarkLifecycleEvent::Progress { message } if message.contains("WARNING: warmup")
        )));
    }

    #[test]
    fn start_emits_completed_lifecycle_after_process_exit() {
        let killed = Arc::new(AtomicBool::new(false));
        let spawner = Arc::new(FakeSpawner::successful(FakeProcess {
            exit_code: 0,
            stdout: Some(Box::new(Cursor::new(
                completed_stdout().as_bytes().to_vec(),
            ))),
            stderr: Some(Box::new(Cursor::new(Vec::<u8>::new()))),
            killed,
        }));
        let runner = RedisBenchmarkRunner::with_spawner(spawner);
        let (sender, receiver) = mpsc::channel();

        let _handle = runner
            .start(sample_request(), sender)
            .expect("runner should start");

        assert!(matches!(
            recv_event(&receiver),
            BenchmarkLifecycleEvent::Started
        ));
        assert!(matches!(
            recv_event(&receiver),
            BenchmarkLifecycleEvent::Progress { .. }
        ));
        match recv_event_matching(&receiver, |event| {
            matches!(event, BenchmarkLifecycleEvent::Completed { .. })
        }) {
            BenchmarkLifecycleEvent::Completed { result } => {
                assert_eq!(result.total_requests, 100000);
                assert_eq!(result.p95_latency_ms, 0.880);
            }
            other => panic!("expected completed event, got {other:?}"),
        }
    }

    #[test]
    fn start_emits_failed_lifecycle_for_missing_binary() {
        let runner = RedisBenchmarkRunner::with_spawner(Arc::new(FakeSpawner::failing(
            "binary_missing",
            "failed to start redis-benchmark: No such file or directory",
        )));
        let (sender, _receiver) = mpsc::channel();

        let result = runner.start(sample_request(), sender);
        assert!(result.is_err());
        let error = result
            .err()
            .expect("start should fail when binary is missing");

        assert_eq!(error.code, "binary_missing");
    }

    #[test]
    fn start_emits_failed_lifecycle_for_early_exit() {
        let spawner = Arc::new(FakeSpawner::successful(FakeProcess {
            exit_code: 2,
            stdout: Some(Box::new(Cursor::new(Vec::<u8>::new()))),
            stderr: Some(Box::new(Cursor::new(b"connection refused".to_vec()))),
            killed: Arc::new(AtomicBool::new(false)),
        }));
        let runner = RedisBenchmarkRunner::with_spawner(spawner);
        let (sender, receiver) = mpsc::channel();

        let _handle = runner
            .start(sample_request(), sender)
            .expect("runner should start");

        let _ = recv_event(&receiver);
        match recv_event_matching(&receiver, |event| {
            matches!(event, BenchmarkLifecycleEvent::Failed { .. })
        }) {
            BenchmarkLifecycleEvent::Failed { message } => {
                assert!(message.contains("exited early"));
                assert!(message.contains("connection refused"));
            }
            other => panic!("expected failed event, got {other:?}"),
        }
    }

    #[test]
    fn stop_emits_cancelled_lifecycle_event() {
        let killed = Arc::new(AtomicBool::new(false));
        let spawner = Arc::new(FakeSpawner::successful(FakeProcess {
            exit_code: -1,
            stdout: Some(Box::new(Cursor::new(Vec::<u8>::new()))),
            stderr: Some(Box::new(Cursor::new(Vec::<u8>::new()))),
            killed: Arc::clone(&killed),
        }));
        let runner = RedisBenchmarkRunner::with_spawner(spawner);
        let (sender, receiver) = mpsc::channel();

        let handle = runner
            .start(sample_request(), sender)
            .expect("runner should start");
        handle.stop().expect("stop should succeed");

        let _ = recv_event(&receiver);
        let _ = recv_event(&receiver);
        assert!(matches!(
            recv_event(&receiver),
            BenchmarkLifecycleEvent::Cancelled
        ));
        assert!(killed.load(Ordering::SeqCst));
    }

    #[test]
    fn malformed_output_becomes_structured_failure() {
        let spawner = Arc::new(FakeSpawner::successful(FakeProcess {
            exit_code: 0,
            stdout: Some(Box::new(Cursor::new(b"nonsense".to_vec()))),
            stderr: Some(Box::new(Cursor::new(b"broken pipe".to_vec()))),
            killed: Arc::new(AtomicBool::new(false)),
        }));
        let runner = RedisBenchmarkRunner::with_spawner(spawner);
        let (sender, receiver) = mpsc::channel();

        let _handle = runner
            .start(sample_request(), sender)
            .expect("runner should start");

        let _ = recv_event(&receiver);
        match recv_event_matching(&receiver, |event| {
            matches!(event, BenchmarkLifecycleEvent::Failed { .. })
        }) {
            BenchmarkLifecycleEvent::Failed { message } => {
                assert!(message.contains("failed to parse redis-benchmark output"));
                assert!(message.contains("broken pipe"));
            }
            other => panic!("expected failed event, got {other:?}"),
        }
    }

    fn recv_event_matching<F>(
        receiver: &mpsc::Receiver<BenchmarkLifecycleEvent>,
        predicate: F,
    ) -> BenchmarkLifecycleEvent
    where
        F: Fn(&BenchmarkLifecycleEvent) -> bool,
    {
        loop {
            let event = recv_event(receiver);
            if predicate(&event) {
                return event;
            }
        }
    }
}
