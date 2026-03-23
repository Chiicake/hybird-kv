use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::{InfoSnapshot, ServerStatus, StartServerRequest};

const DEFAULT_ADDRESS: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 6380;
const EARLY_EXIT_WINDOW: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
}

pub trait ManagedChild: Send {
    fn id(&self) -> u32;
    fn try_wait(&mut self) -> std::io::Result<Option<i32>>;
    fn kill(&mut self) -> std::io::Result<()>;
    fn wait(&mut self) -> std::io::Result<()>;
}

pub trait ProcessLauncher: Send + Sync {
    fn spawn(&self, spec: &LaunchSpec) -> Result<Box<dyn ManagedChild>, String>;
}

#[derive(Debug)]
pub struct SystemLauncher {
    binary_path: PathBuf,
}

impl SystemLauncher {
    pub fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }
}

impl ProcessLauncher for SystemLauncher {
    fn spawn(&self, spec: &LaunchSpec) -> Result<Box<dyn ManagedChild>, String> {
        let mut command = Command::new(&spec.program);
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::piped());

        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let child = command.spawn().map_err(|err| {
            format!(
                "failed to start hkv-server from {}: {}",
                self.binary_path.display(),
                err
            )
        })?;

        Ok(Box::new(SystemChild { inner: child }))
    }
}

#[derive(Debug)]
struct SystemChild {
    inner: Child,
}

impl ManagedChild for SystemChild {
    fn id(&self) -> u32 {
        self.inner.id()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
        self.inner
            .try_wait()
            .map(|status| status.map(|value| value.code().unwrap_or(-1)))
    }

    fn kill(&mut self) -> std::io::Result<()> {
        self.inner.kill()
    }

    fn wait(&mut self) -> std::io::Result<()> {
        let _ = self.inner.wait()?;
        Ok(())
    }
}

struct ManagedProcess {
    child: Box<dyn ManagedChild>,
    address: String,
    started_at: String,
    pid: u32,
}

pub struct ServerManager {
    launcher: Box<dyn ProcessLauncher>,
    state: Mutex<ManagerState>,
}

struct ManagerState {
    process: Option<ManagedProcess>,
    last_error: Option<String>,
    last_launch_spec: Option<LaunchSpec>,
}

impl Default for ServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerManager {
    pub fn new() -> Self {
        let binary_path = default_server_binary_path();
        Self::with_launcher(Box::new(SystemLauncher::new(binary_path)))
    }

    pub fn with_launcher(launcher: Box<dyn ProcessLauncher>) -> Self {
        Self {
            launcher,
            state: Mutex::new(ManagerState {
                process: None,
                last_error: None,
                last_launch_spec: None,
            }),
        }
    }

    pub fn start(&self, request: Option<StartServerRequest>) -> Result<ServerStatus, String> {
        let request = request.unwrap_or_else(default_request);
        let address = format!("{}:{}", request.address, request.port);
        let spec = LaunchSpec {
            program: default_server_binary_path(),
            env: vec![("HKV_ADDR".into(), address.clone())],
        };

        let mut state = self.state.lock().expect("server manager mutex poisoned");

        if let Some(process) = state.process.as_mut() {
            match process.child.try_wait() {
                Ok(None) => {
                    return Ok(ServerStatus {
                        state: "running".into(),
                        address: process.address.clone(),
                        pid: Some(process.pid),
                        started_at: Some(process.started_at.clone()),
                        last_error: state.last_error.clone(),
                    });
                }
                Ok(Some(_)) => {
                    state.process = None;
                }
                Err(err) => {
                    let message = format!("failed to inspect hkv-server state: {err}");
                    state.last_error = Some(message.clone());
                    return Err(message);
                }
            }
        }

        state.last_launch_spec = Some(spec.clone());
        let mut child = self.launcher.spawn(&spec)?;
        let pid = child.id();
        std::thread::sleep(EARLY_EXIT_WINDOW);

        if let Some(code) = child
            .try_wait()
            .map_err(|err| format!("failed to inspect hkv-server startup: {err}"))?
        {
            let message = format!("hkv-server exited before becoming ready (exit code {code})");
            state.last_error = Some(message.clone());
            return Err(message);
        }

        let started_at = iso_timestamp(SystemTime::now());
        state.last_error = None;
        state.process = Some(ManagedProcess {
            child,
            address: address.clone(),
            started_at: started_at.clone(),
            pid,
        });

        Ok(ServerStatus {
            state: "running".into(),
            address,
            pid: Some(pid),
            started_at: Some(started_at),
            last_error: None,
        })
    }

    pub fn stop(&self) -> Result<ServerStatus, String> {
        let mut state = self.state.lock().expect("server manager mutex poisoned");

        let Some(mut process) = state.process.take() else {
            return Ok(ServerStatus {
                state: "stopped".into(),
                address: default_address_string(),
                pid: None,
                started_at: None,
                last_error: state.last_error.clone(),
            });
        };

        if let Some(code) = process
            .child
            .try_wait()
            .map_err(|err| format!("failed to inspect hkv-server state: {err}"))?
        {
            state.last_error = Some(format!("hkv-server already exited (exit code {code})"));
        } else {
            process
                .child
                .kill()
                .map_err(|err| format!("failed to stop hkv-server: {err}"))?;
            process
                .child
                .wait()
                .map_err(|err| format!("failed to reap hkv-server: {err}"))?;
        }

        Ok(ServerStatus {
            state: "stopped".into(),
            address: process.address,
            pid: None,
            started_at: None,
            last_error: state.last_error.clone(),
        })
    }

    pub fn status(&self) -> ServerStatus {
        let mut state = self.state.lock().expect("server manager mutex poisoned");
        let mut finished_address = None;

        if let Some(process) = state.process.as_mut() {
            let address = process.address.clone();
            let pid = process.pid;
            let started_at = process.started_at.clone();

            match process.child.try_wait() {
                Ok(None) => {
                    return ServerStatus {
                        state: "running".into(),
                        address,
                        pid: Some(pid),
                        started_at: Some(started_at),
                        last_error: state.last_error.clone(),
                    };
                }
                Ok(Some(code)) => {
                    state.last_error =
                        Some(format!("hkv-server exited unexpectedly (exit code {code})"));
                    finished_address = Some(address);
                }
                Err(err) => {
                    let message = format!("failed to inspect hkv-server state: {err}");
                    state.last_error = Some(message);
                }
            }
        }

        if let Some(address) = finished_address {
            state.process = None;
            return ServerStatus {
                state: "stopped".into(),
                address,
                pid: None,
                started_at: None,
                last_error: state.last_error.clone(),
            };
        }

        ServerStatus {
            state: "stopped".into(),
            address: state
                .process
                .as_ref()
                .map(|process| process.address.clone())
                .unwrap_or_else(default_address_string),
            pid: None,
            started_at: None,
            last_error: state.last_error.clone(),
        }
    }
}

pub fn parse_info_snapshot(captured_at: &str, raw: &str) -> Result<InfoSnapshot, String> {
    let mut role = None;
    let mut requests_total = 0u64;
    let mut qps_avg = 0.0f64;
    let mut uptime_seconds = 0u64;
    let mut connected_clients = 0u64;
    let mut used_memory = 0u64;
    let mut keyspace_hits = 0u64;
    let mut keyspace_misses = 0u64;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };

        match key {
            "role" => role = Some(value.to_string()),
            "connected_clients" => {
                connected_clients = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid connected_clients value '{value}': {err}"))?
            }
            "used_memory" => {
                used_memory = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid used_memory value '{value}': {err}"))?
            }
            "requests_total" => {
                requests_total = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid requests_total value '{value}': {err}"))?
            }
            "total_commands_processed" => {
                requests_total = value.parse::<u64>().map_err(|err| {
                    format!("invalid total_commands_processed value '{value}': {err}")
                })?
            }
            "qps_avg" => {
                qps_avg = value
                    .parse::<f64>()
                    .map_err(|err| format!("invalid qps_avg value '{value}': {err}"))?
            }
            "instantaneous_ops_per_sec" => {
                qps_avg = value.parse::<f64>().map_err(|err| {
                    format!("invalid instantaneous_ops_per_sec value '{value}': {err}")
                })?
            }
            "uptime_sec" => {
                let parsed = value
                    .parse::<f64>()
                    .map_err(|err| format!("invalid uptime_sec value '{value}': {err}"))?;
                uptime_seconds = parsed.floor() as u64;
            }
            "uptime_in_seconds" => {
                uptime_seconds = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid uptime_in_seconds value '{value}': {err}"))?
            }
            "keyspace_hits" => {
                keyspace_hits = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid keyspace_hits value '{value}': {err}"))?
            }
            "keyspace_misses" => {
                keyspace_misses = value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid keyspace_misses value '{value}': {err}"))?
            }
            _ => {}
        }
    }

    Ok(InfoSnapshot {
        captured_at: captured_at.into(),
        role: role.ok_or_else(|| "INFO output missing role".to_string())?,
        connected_clients,
        used_memory,
        total_commands_processed: requests_total,
        instantaneous_ops_per_sec: qps_avg.round() as u64,
        keyspace_hits,
        keyspace_misses,
        uptime_seconds,
    })
}

fn default_request() -> StartServerRequest {
    StartServerRequest {
        address: DEFAULT_ADDRESS.into(),
        port: DEFAULT_PORT,
    }
}

fn default_address_string() -> String {
    format!("{}:{}", DEFAULT_ADDRESS, DEFAULT_PORT)
}

fn default_server_binary_path() -> PathBuf {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root should exist");
    workspace_root
        .join("target")
        .join("debug")
        .join(server_binary_name())
}

fn server_binary_name() -> &'static str {
    if cfg!(windows) {
        "hkv-server.exe"
    } else {
        "hkv-server"
    }
}

fn iso_timestamp(time: SystemTime) -> String {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    format!("{}Z", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::{parse_info_snapshot, LaunchSpec, ManagedChild, ProcessLauncher, ServerManager};
    use crate::models::StartServerRequest;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeLauncher {
        specs: Arc<Mutex<Vec<LaunchSpec>>>,
        starts: Arc<Mutex<VecDeque<Result<FakeChild, String>>>>,
    }

    impl FakeLauncher {
        fn new(starts: Vec<Result<FakeChild, String>>) -> Self {
            Self {
                specs: Arc::new(Mutex::new(Vec::new())),
                starts: Arc::new(Mutex::new(starts.into())),
            }
        }

        fn specs(&self) -> Vec<LaunchSpec> {
            self.specs.lock().expect("specs mutex poisoned").clone()
        }
    }

    impl ProcessLauncher for FakeLauncher {
        fn spawn(&self, spec: &LaunchSpec) -> Result<Box<dyn ManagedChild>, String> {
            self.specs
                .lock()
                .expect("specs mutex poisoned")
                .push(spec.clone());
            match self
                .starts
                .lock()
                .expect("starts mutex poisoned")
                .pop_front()
            {
                Some(Ok(child)) => Ok(Box::new(child)),
                Some(Err(err)) => Err(err),
                None => Err("no fake process configured".into()),
            }
        }
    }

    struct FakeChild {
        pid: u32,
        waits: VecDeque<Result<Option<i32>, std::io::Error>>,
        kill_called: bool,
        wait_called: bool,
    }

    impl FakeChild {
        fn running(pid: u32) -> Self {
            Self {
                pid,
                waits: vec![Ok(None), Ok(None), Ok(None)].into(),
                kill_called: false,
                wait_called: false,
            }
        }

        fn exited(pid: u32, code: i32) -> Self {
            Self {
                pid,
                waits: vec![Ok(Some(code))].into(),
                kill_called: false,
                wait_called: false,
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
            self.kill_called = true;
            Ok(())
        }

        fn wait(&mut self) -> std::io::Result<()> {
            self.wait_called = true;
            Ok(())
        }
    }

    #[test]
    fn start_command_tracks_running_process_metadata() {
        let launcher = FakeLauncher::new(vec![Ok(FakeChild::running(4242))]);
        let manager = ServerManager::with_launcher(Box::new(launcher.clone()));

        let status = manager
            .start(Some(StartServerRequest {
                address: "127.0.0.1".into(),
                port: 6380,
            }))
            .expect("start should succeed");

        assert_eq!(status.state, "running");
        assert_eq!(status.address, "127.0.0.1:6380");
        assert_eq!(status.pid, Some(4242));
        assert!(status.started_at.is_some());

        let specs = launcher.specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].env,
            vec![("HKV_ADDR".into(), "127.0.0.1:6380".into())]
        );
        assert!(specs[0].program.ends_with("target/debug/hkv-server"));
    }

    #[test]
    fn stop_without_process_state_is_safe() {
        let launcher = FakeLauncher::new(Vec::new());
        let manager = ServerManager::with_launcher(Box::new(launcher));
        let status = manager.stop().expect("stop should succeed");

        assert_eq!(status.state, "stopped");
        assert_eq!(status.address, "127.0.0.1:6380");
        assert!(status.last_error.is_none());
    }

    #[test]
    fn status_reflects_running_and_stopped_states() {
        let launcher = FakeLauncher::new(vec![Ok(FakeChild::running(5001))]);
        let manager = ServerManager::with_launcher(Box::new(launcher));
        assert_eq!(manager.status().state, "stopped");

        let running = manager
            .start(Some(StartServerRequest {
                address: "127.0.0.1".into(),
                port: 6381,
            }))
            .expect("start should succeed");

        assert_eq!(running.state, "running");
        assert_eq!(manager.status().state, "running");

        manager.stop().expect("stop should succeed");
        assert_eq!(manager.status().state, "stopped");
    }

    #[test]
    fn info_parsing_normalizes_hkv_server_output() {
        let snapshot = parse_info_snapshot(
            "2026-03-23T12:00:00Z",
            concat!(
                "role:master\r\n",
                "connected_clients:3\r\n",
                "used_memory:4096\r\n",
                "engine:hybridkv\r\n",
                "requests_total:42\r\n",
                "errors_total:1\r\n",
                "inflight:0\r\n",
                "uptime_sec:12.800\r\n",
                "qps_avg:17.900\r\n",
                "error_rate:0.023\r\n",
                "keyspace_hits:11\r\n",
                "keyspace_misses:2\r\n",
                "latency_samples:42\r\n",
                "latency_avg_us:99.000\r\n",
                "latency_max_us:100\r\n",
                "latency_p50_us:90\r\n",
                "latency_p90_us:98\r\n",
                "latency_p99_us:100\r\n",
                "latency_p999_us:100\r\n"
            ),
        )
        .expect("INFO should parse");

        assert_eq!(snapshot.captured_at, "2026-03-23T12:00:00Z");
        assert_eq!(snapshot.role, "master");
        assert_eq!(snapshot.total_commands_processed, 42);
        assert_eq!(snapshot.instantaneous_ops_per_sec, 18);
        assert_eq!(snapshot.uptime_seconds, 12);
        assert_eq!(snapshot.connected_clients, 3);
        assert_eq!(snapshot.used_memory, 4096);
        assert_eq!(snapshot.keyspace_hits, 11);
        assert_eq!(snapshot.keyspace_misses, 2);
    }

    #[test]
    fn missing_binary_or_early_exit_surfaces_user_facing_errors() {
        let launcher = FakeLauncher::new(vec![Err(
            "failed to start hkv-server: binary unavailable".into(),
        )]);
        let manager = ServerManager::with_launcher(Box::new(launcher));

        let err = manager
            .start(Some(StartServerRequest {
                address: "127.0.0.1".into(),
                port: 6399,
            }))
            .expect_err("start should fail when binary is unavailable");

        assert!(err.contains("hkv-server"));

        let launcher = FakeLauncher::new(vec![Ok(FakeChild::exited(7001, 9))]);
        let manager = ServerManager::with_launcher(Box::new(launcher));
        let err = manager
            .start(Some(StartServerRequest {
                address: "127.0.0.1".into(),
                port: 6400,
            }))
            .expect_err("start should fail when process exits early");

        assert!(err.contains("exited before becoming ready"));
    }
}
