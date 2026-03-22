use serde::Serialize;

use crate::models::{
    BenchmarkRun, BenchmarkRunRequest, InfoSnapshot, NormalizedRunSummary, ServerStatus,
    StartServerRequest,
};
use crate::state::AppState;

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
    fn not_implemented(message: &str) -> Self {
        Self {
            code: "not_implemented".into(),
            message: message.into(),
        }
    }
}

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
    _state: tauri::State<'_, AppState>,
    request: BenchmarkRunRequest,
) -> Result<BenchmarkRun, ApiError> {
    let _ = request;

    Err(ApiError::not_implemented(
        "benchmark orchestration is not implemented yet",
    ))
}

#[tauri::command]
pub async fn stop_benchmark(
    _state: tauri::State<'_, AppState>,
    run_id: String,
) -> Result<BenchmarkRun, ApiError> {
    let _ = run_id;

    Err(ApiError::not_implemented(
        "benchmark orchestration is not implemented yet",
    ))
}

#[tauri::command]
pub async fn list_runs(state: tauri::State<'_, AppState>) -> Result<Vec<NormalizedRunSummary>, ApiError> {
    Ok(state.list_runs())
}

#[tauri::command]
pub async fn get_run_detail(
    _state: tauri::State<'_, AppState>,
    run_id: String,
) -> Result<BenchmarkRun, ApiError> {
    let _ = run_id;

    Err(ApiError::not_implemented(
        "run detail persistence is not implemented yet",
    ))
}

#[tauri::command]
pub async fn start_server(
    _state: tauri::State<'_, AppState>,
    request: Option<StartServerRequest>,
) -> Result<ServerStatus, ApiError> {
    let _ = request;

    Err(ApiError::not_implemented(
        "server lifecycle management is not implemented yet",
    ))
}

#[tauri::command]
pub async fn stop_server(_state: tauri::State<'_, AppState>) -> Result<ServerStatus, ApiError> {
    Err(ApiError::not_implemented(
        "server lifecycle management is not implemented yet",
    ))
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
    use super::{
        command_names, register_commands, ApiError, APP_COMMAND_NAMES,
    };
    use crate::models::{
        BenchmarkEventEnvelope, ServerEventEnvelope, BENCHMARK_EVENT_CHANNEL,
        SERVER_EVENT_CHANNEL,
    };
    use crate::state::AppState;
    use serde_json::Value;

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
        let state = AppState::default();
        let benchmark_err = ApiError::not_implemented("benchmark orchestration is not implemented yet");
        let run_detail_err = ApiError::not_implemented("run detail persistence is not implemented yet");
        let server_err = ApiError::not_implemented("server lifecycle management is not implemented yet");

        assert_eq!(benchmark_err.code, "not_implemented");
        assert_eq!(benchmark_err.message, "benchmark orchestration is not implemented yet");

        let list = state.list_runs();
        assert!(list.is_empty());

        assert_eq!(run_detail_err.code, "not_implemented");
        assert_eq!(server_err.code, "not_implemented");

        let status = state.server_status();
        assert_eq!(status.state, "stopped");
        assert_eq!(status.address, "127.0.0.1:6380");

        let info = state.info_snapshot();
        assert!(info.is_none());
    }

    #[test]
    fn event_contracts_match_frontend_consumable_envelopes() {
        let benchmark_event = BenchmarkEventEnvelope {
            channel: BENCHMARK_EVENT_CHANNEL.into(),
            event: "queued".into(),
            run_id: "run-001".into(),
            emitted_at: "2026-03-22T10:00:01Z".into(),
        };

        let server_event = ServerEventEnvelope {
            channel: SERVER_EVENT_CHANNEL.into(),
            event: "state-changed".into(),
            emitted_at: "2026-03-22T10:00:02Z".into(),
            status: AppState::default().server_status(),
            info: None,
        };

        let benchmark_json = serde_json::to_value(&benchmark_event).expect("event should serialize");
        let server_json = serde_json::to_value(&server_event).expect("event should serialize");

        assert_eq!(benchmark_json["channel"], BENCHMARK_EVENT_CHANNEL);
        assert_eq!(benchmark_json["runId"], "run-001");
        assert_eq!(server_json["channel"], SERVER_EVENT_CHANNEL);
        assert_eq!(server_json["status"]["state"], "stopped");
        assert!(server_json["info"].is_null());
    }
}
