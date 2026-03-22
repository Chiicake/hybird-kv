export const BENCHMARK_EVENT_CHANNEL = "benchmark:lifecycle";
export const SERVER_EVENT_CHANNEL = "server:status";

export type BenchmarkRunRequest = {
  runner: string;
  targetAddr: string;
  clients: number;
  requests: number;
  dataSize: number;
  pipeline: number;
};

export type BenchmarkResult = {
  totalRequests: number;
  throughputOpsPerSec: number;
  averageLatencyMs: number;
  p50LatencyMs: number;
  p95LatencyMs: number;
  p99LatencyMs: number;
  durationMs: number;
  datasetBytes: number;
};

export type BenchmarkRun = {
  id: string;
  request: BenchmarkRunRequest;
  status: string;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
  result: BenchmarkResult | null;
  errorMessage: string | null;
};

export type NormalizedRunSummary = {
  id: string;
  runner: string;
  status: string;
  targetAddr: string;
  createdAt: string;
  finishedAt: string | null;
  throughputOpsPerSec: number | null;
  p95LatencyMs: number | null;
};

export type ServerStatus = {
  state: string;
  address: string;
  pid: number | null;
  startedAt: string | null;
  lastError: string | null;
};

export type InfoSnapshot = {
  capturedAt: string;
  role: string;
  connectedClients: number;
  usedMemory: number;
  totalCommandsProcessed: number;
  instantaneousOpsPerSec: number;
  keyspaceHits: number;
  keyspaceMisses: number;
  uptimeSeconds: number;
};

export type StartServerRequest = {
  address: string;
  port: number;
};

export type BenchmarkEventEnvelope = {
  channel: string;
  event: string;
  runId: string;
  emittedAt: string;
};

export type ServerEventEnvelope = {
  channel: string;
  event: string;
  emittedAt: string;
  status: ServerStatus;
  info: InfoSnapshot | null;
};

export type ApiError = {
  code: string;
  message: string;
};
