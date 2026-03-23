use serde::{Deserialize, Serialize};

pub const BENCHMARK_EVENT_CHANNEL: &str = "benchmark:lifecycle";
#[cfg(test)]
pub const SERVER_EVENT_CHANNEL: &str = "server:status";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRunRequest {
    pub runner: String,
    pub target_addr: String,
    pub clients: u32,
    pub requests: u64,
    pub data_size: u32,
    pub pipeline: u32,
}

impl BenchmarkRunRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.runner.trim().is_empty() {
            return Err("runner must not be empty".into());
        }

        if self.target_addr.trim().is_empty() || !self.target_addr.contains(':') {
            return Err("target_addr must use host:port format".into());
        }

        if self.clients == 0 || self.requests == 0 || self.data_size == 0 || self.pipeline == 0 {
            return Err(
                "clients, requests, data_size, and pipeline must be greater than zero".into(),
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResult {
    pub total_requests: u64,
    pub throughput_ops_per_sec: f64,
    pub average_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub duration_ms: u64,
    pub dataset_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRun {
    pub id: String,
    pub request: BenchmarkRunRequest,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub result: Option<BenchmarkResult>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedRunSummary {
    pub id: String,
    pub runner: String,
    pub status: String,
    pub target_addr: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub throughput_ops_per_sec: Option<f64>,
    pub p95_latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub state: String,
    pub address: String,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InfoSnapshot {
    pub captured_at: String,
    pub role: String,
    pub connected_clients: u64,
    pub used_memory: u64,
    pub total_commands_processed: u64,
    pub instantaneous_ops_per_sec: u64,
    pub keyspace_hits: u64,
    pub keyspace_misses: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartServerRequest {
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkEventEnvelope {
    pub channel: String,
    pub event: String,
    pub run_id: String,
    pub emitted_at: String,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerEventEnvelope {
    pub channel: String,
    pub event: String,
    pub emitted_at: String,
    pub status: ServerStatus,
    pub info: Option<InfoSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::{
        BenchmarkEventEnvelope, BenchmarkResult, BenchmarkRun, BenchmarkRunRequest, InfoSnapshot,
        NormalizedRunSummary, ServerEventEnvelope, ServerStatus, StartServerRequest,
        BENCHMARK_EVENT_CHANNEL, SERVER_EVENT_CHANNEL,
    };
    use serde::Serialize;
    use serde_json::Value;

    fn contract_schema() -> Value {
        serde_json::from_str(include_str!("../../src/lib/contract-schema.json"))
            .expect("contract schema should parse")
    }

    fn object_keys<T: Serialize>(value: &T) -> Vec<String> {
        let mut keys = serde_json::to_value(value)
            .expect("value should serialize")
            .as_object()
            .expect("serialized value should be an object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    fn schema_keys(schema: &Value, model_name: &str) -> Vec<String> {
        let mut keys = schema["models"][model_name]
            .as_array()
            .expect("schema model should be an array")
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("schema key should be a string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    #[test]
    fn benchmark_run_request_serializes_with_stable_shape() {
        let schema = contract_schema();
        let request = BenchmarkRunRequest {
            runner: "redis-benchmark".into(),
            target_addr: "127.0.0.1:6379".into(),
            clients: 32,
            requests: 100_000,
            data_size: 128,
            pipeline: 4,
        };

        let json = serde_json::to_value(&request).expect("request should serialize");

        assert_eq!(json["runner"], "redis-benchmark");
        assert_eq!(json["targetAddr"], "127.0.0.1:6379");
        assert_eq!(json["clients"], 32);
        assert_eq!(json["requests"], 100_000);
        assert_eq!(json["dataSize"], 128);
        assert_eq!(json["pipeline"], 4);
        assert_eq!(
            object_keys(&request),
            schema_keys(&schema, "benchmarkRunRequest")
        );
        assert!(request.validate().is_ok());
    }

    #[test]
    fn benchmark_run_request_validation_rejects_invalid_values() {
        let request = BenchmarkRunRequest {
            runner: String::new(),
            target_addr: "127.0.0.1".into(),
            clients: 0,
            requests: 0,
            data_size: 0,
            pipeline: 0,
        };

        assert_eq!(
            request.validate().expect_err("request should be invalid"),
            "runner must not be empty"
        );
    }

    #[test]
    fn benchmark_run_and_result_capture_contract_fields() {
        let schema = contract_schema();
        let run = BenchmarkRun {
            id: "run-001".into(),
            request: BenchmarkRunRequest {
                runner: "redis-benchmark".into(),
                target_addr: "127.0.0.1:6379".into(),
                clients: 50,
                requests: 500_000,
                data_size: 256,
                pipeline: 8,
            },
            status: "queued".into(),
            created_at: "2026-03-22T10:00:00Z".into(),
            started_at: None,
            finished_at: None,
            result: Some(BenchmarkResult {
                total_requests: 500_000,
                throughput_ops_per_sec: 125_000.0,
                average_latency_ms: 1.8,
                p50_latency_ms: 1.1,
                p95_latency_ms: 2.6,
                p99_latency_ms: 4.9,
                duration_ms: 4_000,
                dataset_bytes: 131_072,
            }),
            error_message: None,
        };

        let json = serde_json::to_value(&run).expect("run should serialize");

        assert_eq!(json["id"], "run-001");
        assert_eq!(json["status"], "queued");
        assert_eq!(json["result"]["throughputOpsPerSec"], 125_000.0);
        assert_eq!(json["result"]["p99LatencyMs"], 4.9);
        assert_eq!(object_keys(&run), schema_keys(&schema, "benchmarkRun"));
        assert_eq!(
            object_keys(run.result.as_ref().expect("result should exist")),
            schema_keys(&schema, "benchmarkResult")
        );
    }

    #[test]
    fn normalized_summary_and_server_models_use_frontend_safe_keys() {
        let schema = contract_schema();
        let summary = NormalizedRunSummary {
            id: "run-002".into(),
            runner: "redis-benchmark".into(),
            status: "completed".into(),
            target_addr: "127.0.0.1:6379".into(),
            created_at: "2026-03-22T10:05:00Z".into(),
            finished_at: Some("2026-03-22T10:05:04Z".into()),
            throughput_ops_per_sec: Some(130_000.0),
            p95_latency_ms: Some(2.2),
        };

        let server_status = ServerStatus {
            state: "stopped".into(),
            address: "127.0.0.1:6380".into(),
            pid: None,
            started_at: None,
            last_error: Some("not started".into()),
        };

        let summary_json = serde_json::to_value(&summary).expect("summary should serialize");
        let status_json = serde_json::to_value(&server_status).expect("status should serialize");

        assert_eq!(summary_json["throughputOpsPerSec"], 130_000.0);
        assert_eq!(summary_json["p95LatencyMs"], 2.2);
        assert_eq!(status_json["lastError"], "not started");
        assert_eq!(status_json["address"], "127.0.0.1:6380");
        assert_eq!(
            object_keys(&summary),
            schema_keys(&schema, "normalizedRunSummary")
        );
        assert_eq!(
            object_keys(&server_status),
            schema_keys(&schema, "serverStatus")
        );
    }

    #[test]
    fn info_snapshot_and_event_envelopes_define_consistent_payloads() {
        let schema = contract_schema();
        let info = InfoSnapshot {
            captured_at: "2026-03-22T10:06:00Z".into(),
            role: "master".into(),
            connected_clients: 3,
            used_memory: 4_096,
            total_commands_processed: 90,
            instantaneous_ops_per_sec: 45,
            keyspace_hits: 11,
            keyspace_misses: 2,
            uptime_seconds: 120,
        };

        let benchmark_event = BenchmarkEventEnvelope {
            channel: BENCHMARK_EVENT_CHANNEL.into(),
            event: "queued".into(),
            run_id: "run-003".into(),
            emitted_at: "2026-03-22T10:06:01Z".into(),
            message: Some("queued for execution".into()),
            error: None,
        };

        let server_event = ServerEventEnvelope {
            channel: SERVER_EVENT_CHANNEL.into(),
            event: "state-changed".into(),
            emitted_at: "2026-03-22T10:06:02Z".into(),
            status: ServerStatus {
                state: "running".into(),
                address: "127.0.0.1:6380".into(),
                pid: Some(4242),
                started_at: Some("2026-03-22T10:05:59Z".into()),
                last_error: None,
            },
            info: Some(info.clone()),
        };

        let benchmark_json =
            serde_json::to_value(&benchmark_event).expect("event should serialize");
        let server_json = serde_json::to_value(&server_event).expect("event should serialize");

        assert_eq!(benchmark_json["channel"], BENCHMARK_EVENT_CHANNEL);
        assert_eq!(benchmark_json["runId"], "run-003");
        assert_eq!(benchmark_json["message"], "queued for execution");
        assert!(benchmark_json["error"].is_null());
        assert_eq!(server_json["info"]["instantaneousOpsPerSec"], 45);
        assert_eq!(server_json["status"]["pid"], 4242);
        assert_eq!(object_keys(&info), schema_keys(&schema, "infoSnapshot"));
        assert_eq!(
            object_keys(&benchmark_event),
            schema_keys(&schema, "benchmarkEventEnvelope")
        );
        assert_eq!(
            object_keys(&server_event),
            schema_keys(&schema, "serverEventEnvelope")
        );
        assert_eq!(schema["channels"]["benchmark"], BENCHMARK_EVENT_CHANNEL);
        assert_eq!(schema["channels"]["server"], SERVER_EVENT_CHANNEL);
    }

    #[test]
    fn start_server_request_contract_matches_shared_schema() {
        let schema = contract_schema();
        let request = StartServerRequest {
            address: "127.0.0.1".into(),
            port: 6380,
        };

        assert_eq!(
            object_keys(&request),
            schema_keys(&schema, "startServerRequest")
        );
    }
}
