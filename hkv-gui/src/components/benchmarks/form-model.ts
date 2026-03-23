import type { BenchmarkRunRequest } from "../../lib/types";
import { loadRuntimePreferences, resolveBenchmarkBinaryPath } from "../../lib/runtime-preferences";

export type BenchmarkFormValues = {
  host: string;
  port: string;
  executionMode: string;
  profile: string;
  requests: string;
  durationSeconds: string;
  clients: string;
  dataSize: string;
  pipeline: string;
};

export const DEFAULT_BENCHMARK_FORM: BenchmarkFormValues = {
  host: "127.0.0.1",
  port: "6379",
  executionMode: "requests",
  profile: "balanced",
  requests: "200000",
  durationSeconds: "60",
  clients: "32",
  dataSize: "128",
  pipeline: "4"
};

export function createBenchmarkFormDefaults(): BenchmarkFormValues {
  const preferences = loadRuntimePreferences();

  return {
    ...DEFAULT_BENCHMARK_FORM,
    host: preferences.benchmarkTargetHost,
    port: preferences.benchmarkTargetPort
  };
}

export function applyBenchmarkProfile(profile: string, values: BenchmarkFormValues) {
  switch (profile) {
    case "throughput":
      return { ...values, requests: values.requests, clients: "48", pipeline: "8" };
    case "latency":
      return { ...values, requests: values.requests, clients: "12", pipeline: "1" };
    default:
      return values;
  }
}

export function validateBenchmarkForm(values: BenchmarkFormValues) {
  const errors: Partial<Record<keyof BenchmarkFormValues, string>> = {};

  if (!values.host.trim()) {
    errors.host = "Host is required.";
  }

  const port = Number(values.port);
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    errors.port = "Port must be between 1 and 65535.";
  }

  const positiveFields: Array<[keyof BenchmarkFormValues, string]> = [
    ["clients", "Concurrency must be greater than zero."],
    ["dataSize", "Payload size must be greater than zero."],
    ["pipeline", "Pipeline depth must be greater than zero."]
  ];

  positiveFields.forEach(([field, message]) => {
    if ((Number(values[field]) || 0) <= 0) {
      errors[field] = message;
    }
  });

  if (values.executionMode === "duration") {
    if ((Number(values.durationSeconds) || 0) <= 0) {
      errors.durationSeconds = "Duration must be greater than zero.";
    }
  } else if ((Number(values.requests) || 0) <= 0) {
    errors.requests = "Request count must be greater than zero.";
  }

  return errors;
}

export function buildBenchmarkRequest(values: BenchmarkFormValues): BenchmarkRunRequest {
  const requestBudget =
    values.executionMode === "duration"
      ? Math.max(Number(values.durationSeconds) || 0, 1) * 5000
      : Number(values.requests);

  return {
    runner: resolveBenchmarkBinaryPath(loadRuntimePreferences()),
    targetAddr: `${values.host}:${values.port}`,
    clients: Number(values.clients),
    requests: requestBudget,
    dataSize: Number(values.dataSize),
    pipeline: Number(values.pipeline)
  };
}
