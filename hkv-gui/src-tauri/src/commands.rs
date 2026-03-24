use serde::Serialize;

use crate::models::{
    BenchmarkRun, BenchmarkRunRequest, InfoSnapshot, NormalizedRunSummary, ServerStatus,
    StartServerRequest,
};
use crate::state::AppState;

#[cfg(test)]
pub const APP_COMMAND_NAMES: &[&str; 8] = &[
    "start_benchmark",
    "stop_benchmark",
    "list_runs",
    "get_run_detail",
    "start_server",
    "stop_server",
    "server_status",
    "current_info_snapshot",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    #[cfg(test)]
    fn not_implemented(message: &str) -> Self {
        Self {
            code: "not_implemented".into(),
            message: message.into(),
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            code: "runtime_error".into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
pub fn command_names() -> &'static [&'static str; 8] {
    APP_COMMAND_NAMES
}

pub fn register_commands(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_benchmark,
            stop_benchmark,
            list_runs,
            get_run_detail,
            start_server,
            stop_server,
            server_status,
            current_info_snapshot,
        ])
}

#[tauri::command]
pub async fn start_benchmark(
    state: tauri::State<'_, AppState>,
    request: BenchmarkRunRequest,
) -> Result<BenchmarkRun, ApiError> {
    state.start_benchmark(request).map_err(ApiError::runtime)
}

#[tauri::command]
pub async fn stop_benchmark(
    state: tauri::State<'_, AppState>,
    run_id: String,
) -> Result<BenchmarkRun, ApiError> {
    state.stop_benchmark(&run_id).map_err(ApiError::runtime)
}

#[tauri::command]
pub async fn list_runs(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<NormalizedRunSummary>, ApiError> {
    Ok(state.list_runs())
}

#[tauri::command]
pub async fn get_run_detail(
    state: tauri::State<'_, AppState>,
    run_id: String,
) -> Result<BenchmarkRun, ApiError> {
    state.get_run_detail(&run_id).map_err(ApiError::runtime)
}

#[tauri::command]
pub async fn start_server(
    state: tauri::State<'_, AppState>,
    request: Option<StartServerRequest>,
) -> Result<ServerStatus, ApiError> {
    state.start_server(request).map_err(ApiError::runtime)
}

#[tauri::command]
pub async fn stop_server(state: tauri::State<'_, AppState>) -> Result<ServerStatus, ApiError> {
    state.stop_server().map_err(ApiError::runtime)
}

#[tauri::command]
pub async fn server_status(state: tauri::State<'_, AppState>) -> Result<ServerStatus, ApiError> {
    Ok(state.server_status())
}

#[tauri::command]
pub async fn current_info_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<Option<InfoSnapshot>, ApiError> {
    Ok(state.info_snapshot())
}

#[cfg(test)]
mod tests {
    use super::{command_names, register_commands, ApiError, APP_COMMAND_NAMES};
    use crate::benchmark_manager::BenchmarkManager;
    use crate::info_poller::InfoPoller;
    use crate::models::{
        BenchmarkEventEnvelope, ServerEventEnvelope, BENCHMARK_EVENT_CHANNEL, SERVER_EVENT_CHANNEL,
    };
    use crate::run_repository::RunRepository;
    use crate::runners::redis_benchmark::RedisBenchmarkRunner;
    use crate::server_manager::ServerManager;
    use crate::state::AppState;
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_storage_dir(test_name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hkv-commands-{test_name}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn contract_schema() -> Value {
        serde_json::from_str(include_str!("../../src/lib/contract-schema.json"))
            .expect("contract schema should parse")
    }

    #[test]
    fn command_registration_exposes_expected_surface() {
        let names = command_names();
        let _builder = register_commands(tauri::Builder::default());
        let schema = contract_schema();
        let expected_names = schema["commands"]
            .as_array()
            .expect("commands should be an array")
            .iter()
            .map(|value| value.as_str().expect("command name should be a string"))
            .collect::<Vec<_>>();

        assert_eq!(names, APP_COMMAND_NAMES);
        assert_eq!(names.as_slice(), expected_names);
    }

    #[test]
    fn placeholder_responses_match_current_contract_shapes() {
        let storage_dir = temp_storage_dir("placeholder-contract");
        let state = AppState::with_components(
            BenchmarkManager::new(vec![Arc::new(RedisBenchmarkRunner::new())]),
            Arc::new(
                RunRepository::new(storage_dir.clone()).expect("repository should initialize"),
            ),
            ServerManager::new(),
            InfoPoller::new(),
        );
        let benchmark_err =
            ApiError::not_implemented("benchmark orchestration is not implemented yet");

        assert_eq!(benchmark_err.code, "not_implemented");
        assert_eq!(
            benchmark_err.message,
            "benchmark orchestration is not implemented yet"
        );

        let list = state.list_runs();
        assert!(list.is_empty());

        let status = state.server_status();
        assert_eq!(status.state, "stopped");
        assert_eq!(status.address, "127.0.0.1:6380");

        let info = state.info_snapshot();
        assert!(info.is_none());

        fs::remove_dir_all(storage_dir).expect("temp dir should be removed");
    }

    #[test]
    fn event_contracts_match_frontend_consumable_envelopes() {
        let benchmark_event = BenchmarkEventEnvelope {
            channel: BENCHMARK_EVENT_CHANNEL.into(),
            event: "queued".into(),
            run_id: "run-001".into(),
            emitted_at: "2026-03-22T10:00:01Z".into(),
            message: Some("queued for execution".into()),
            error: None,
        };

        let server_event = ServerEventEnvelope {
            channel: SERVER_EVENT_CHANNEL.into(),
            event: "state-changed".into(),
            emitted_at: "2026-03-22T10:00:02Z".into(),
            status: AppState::default().server_status(),
            info: None,
        };

        let benchmark_json =
            serde_json::to_value(&benchmark_event).expect("event should serialize");
        let server_json = serde_json::to_value(&server_event).expect("event should serialize");

        assert_eq!(benchmark_json["channel"], BENCHMARK_EVENT_CHANNEL);
        assert_eq!(benchmark_json["runId"], "run-001");
        assert_eq!(benchmark_json["message"], "queued for execution");
        assert!(benchmark_json["error"].is_null());
        assert_eq!(server_json["channel"], SERVER_EVENT_CHANNEL);
        assert_eq!(server_json["status"]["state"], "stopped");
        assert!(server_json["info"].is_null());
    }
}
