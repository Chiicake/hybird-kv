import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";

import {
  BENCHMARK_EVENT_CHANNEL,
  SERVER_EVENT_CHANNEL,
  type BenchmarkRun,
  type BenchmarkRunRequest,
  type BenchmarkEventEnvelope,
  type InfoSnapshot,
  type NormalizedRunSummary,
  type ServerEventEnvelope,
  type ServerStatus,
  type StartServerRequest
} from "./types";

export { BENCHMARK_EVENT_CHANNEL, SERVER_EVENT_CHANNEL } from "./types";

export async function invokeCommand<TResponse>(
  command: string,
  args?: Record<string, unknown>
): Promise<TResponse> {
  return invoke<TResponse>(command, args);
}

export async function startBenchmark(request: BenchmarkRunRequest): Promise<BenchmarkRun> {
  return invokeCommand<BenchmarkRun>("start_benchmark", { request });
}

export async function stopBenchmark(runId: string): Promise<BenchmarkRun> {
  return invokeCommand<BenchmarkRun>("stop_benchmark", { runId });
}

export async function listRuns(): Promise<NormalizedRunSummary[]> {
  return invokeCommand<NormalizedRunSummary[]>("list_runs");
}

export async function getRunDetail(runId: string): Promise<BenchmarkRun> {
  return invokeCommand<BenchmarkRun>("get_run_detail", { runId });
}

export async function startServer(request?: StartServerRequest): Promise<ServerStatus> {
  return invokeCommand<ServerStatus>("start_server", request ? { request } : undefined);
}

export async function stopServer(): Promise<ServerStatus> {
  return invokeCommand<ServerStatus>("stop_server");
}

export async function serverStatus(): Promise<ServerStatus> {
  return invokeCommand<ServerStatus>("server_status");
}

export async function currentInfoSnapshot(): Promise<InfoSnapshot | null> {
  return invokeCommand<InfoSnapshot | null>("current_info_snapshot");
}

export async function onBenchmarkEvent(
  handler: (payload: BenchmarkEventEnvelope) => void
): Promise<() => void> {
  return listen<BenchmarkEventEnvelope>(BENCHMARK_EVENT_CHANNEL, (event: Event<BenchmarkEventEnvelope>) => {
    handler(event.payload);
  });
}

export async function onServerEvent(
  handler: (payload: ServerEventEnvelope) => void
): Promise<() => void> {
  return listen<ServerEventEnvelope>(SERVER_EVENT_CHANNEL, (event: Event<ServerEventEnvelope>) => {
    handler(event.payload);
  });
}
