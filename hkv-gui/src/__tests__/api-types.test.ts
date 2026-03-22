import { beforeEach, describe, expect, it, vi } from "vitest";
import contractSchema from "../lib/contract-schema.json";

import {
  BENCHMARK_EVENT_CHANNEL,
  SERVER_EVENT_CHANNEL,
  currentInfoSnapshot,
  getRunDetail,
  listRuns,
  onBenchmarkEvent,
  onServerEvent,
  serverStatus,
  startBenchmark,
  startServer,
  stopBenchmark,
  stopServer
} from "../lib/api";
import type {
  BenchmarkEventEnvelope,
  BenchmarkRun,
  BenchmarkRunRequest,
  InfoSnapshot,
  NormalizedRunSummary,
  ServerEventEnvelope,
  ServerStatus,
  StartServerRequest
} from "../lib/types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn()
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn()
}));

describe("gui api contracts", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("keeps benchmark request and event channel names aligned", () => {
    const request: BenchmarkRunRequest = {
      runner: "redis-benchmark",
      targetAddr: "127.0.0.1:6379",
      clients: 32,
      requests: 100000,
      dataSize: 128,
      pipeline: 4
    };

    const event: BenchmarkEventEnvelope = {
      channel: BENCHMARK_EVENT_CHANNEL,
      event: "queued",
      runId: "run-001",
      emittedAt: "2026-03-22T10:00:01Z"
    };

    expect(request.targetAddr).toBe("127.0.0.1:6379");
    expect(event.channel).toBe(contractSchema.channels.benchmark);
    expect(Object.keys(request).sort()).toEqual(
      [...contractSchema.models.benchmarkRunRequest].sort()
    );
    expect(Object.keys(event).sort()).toEqual(
      [...contractSchema.models.benchmarkEventEnvelope].sort()
    );
  });

  it("keeps server request and event payloads frontend-safe", () => {
    const request: StartServerRequest = {
      address: "127.0.0.1",
      port: 6380
    };

    const event: ServerEventEnvelope = {
      channel: SERVER_EVENT_CHANNEL,
      event: "state-changed",
      emittedAt: "2026-03-22T10:01:00Z",
      status: {
        state: "running",
        address: "127.0.0.1:6380",
        pid: 4242,
        startedAt: "2026-03-22T10:00:59Z",
        lastError: null
      },
      info: null
    };

    expect(request.port).toBe(6380);
    expect(event.status.address).toBe("127.0.0.1:6380");
    expect(Object.keys(request).sort()).toEqual(
      [...contractSchema.models.startServerRequest].sort()
    );
    expect(Object.keys(event.status).sort()).toEqual(
      [...contractSchema.models.serverStatus].sort()
    );
    expect(Object.keys(event).sort()).toEqual(
      [...contractSchema.models.serverEventEnvelope].sort()
    );
  });

  it("keeps shared contract snapshot aligned with frontend command and channel constants", () => {
    expect(contractSchema.commands).toEqual([
      "start_benchmark",
      "stop_benchmark",
      "list_runs",
      "get_run_detail",
      "start_server",
      "stop_server",
      "server_status",
      "current_info_snapshot"
    ]);
    expect(BENCHMARK_EVENT_CHANNEL).toBe(contractSchema.channels.benchmark);
    expect(SERVER_EVENT_CHANNEL).toBe(contractSchema.channels.server);
  });

  it("provides dedicated benchmark and run wrappers over the command surface", async () => {
    const unlisten = vi.fn();
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const runRequest: BenchmarkRunRequest = {
      runner: "redis-benchmark",
      targetAddr: "127.0.0.1:6379",
      clients: 32,
      requests: 100000,
      dataSize: 128,
      pipeline: 4
    };
    const run: BenchmarkRun = {
      id: "run-001",
      request: runRequest,
      status: "queued",
      createdAt: "2026-03-22T10:00:00Z",
      startedAt: null,
      finishedAt: null,
      result: null,
      errorMessage: null
    };
    const summaries: NormalizedRunSummary[] = [
      {
        id: "run-001",
        runner: "redis-benchmark",
        status: "queued",
        targetAddr: "127.0.0.1:6379",
        createdAt: "2026-03-22T10:00:00Z",
        finishedAt: null,
        throughputOpsPerSec: null,
        p95LatencyMs: null
      }
    ];

    vi.mocked(invoke)
      .mockResolvedValueOnce(run)
      .mockResolvedValueOnce(run)
      .mockResolvedValueOnce(summaries)
      .mockResolvedValueOnce(run);
    vi.mocked(listen).mockResolvedValue(unlisten);

    const started = await startBenchmark(runRequest);
    const stopped = await stopBenchmark("run-001");
    const runs = await listRuns();
    const detail = await getRunDetail("run-001");
    const stopBenchmarkListener = await onBenchmarkEvent(() => undefined);
    const stopServerListener = await onServerEvent(() => undefined);

    expect(started).toEqual(run);
    expect(stopped).toEqual(run);
    expect(runs).toEqual(summaries);
    expect(detail).toEqual(run);
    expect(invoke).toHaveBeenNthCalledWith(1, "start_benchmark", { request: runRequest });
    expect(invoke).toHaveBeenNthCalledWith(2, "stop_benchmark", { runId: "run-001" });
    expect(invoke).toHaveBeenNthCalledWith(3, "list_runs", undefined);
    expect(invoke).toHaveBeenNthCalledWith(4, "get_run_detail", { runId: "run-001" });
    expect(listen).toHaveBeenCalledWith(BENCHMARK_EVENT_CHANNEL, expect.any(Function));
    expect(listen).toHaveBeenCalledWith(SERVER_EVENT_CHANNEL, expect.any(Function));

    stopBenchmarkListener();
    stopServerListener();

    expect(unlisten).toHaveBeenCalledTimes(2);
  });

  it("provides dedicated server wrappers over the command surface", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const request: StartServerRequest = {
      address: "127.0.0.1",
      port: 6380
    };
    const status: ServerStatus = {
      state: "stopped",
      address: "127.0.0.1:6380",
      pid: null,
      startedAt: null,
      lastError: null
    };
    const snapshot: InfoSnapshot = {
      capturedAt: "2026-03-22T10:01:00Z",
      role: "master",
      connectedClients: 3,
      usedMemory: 4096,
      totalCommandsProcessed: 90,
      instantaneousOpsPerSec: 45,
      keyspaceHits: 11,
      keyspaceMisses: 2,
      uptimeSeconds: 120
    };

    vi.mocked(invoke)
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(snapshot);

    const started = await startServer(request);
    const stopped = await stopServer();
    const currentStatus = await serverStatus();
    const info = await currentInfoSnapshot();

    expect(started).toEqual(status);
    expect(stopped).toEqual(status);
    expect(currentStatus).toEqual(status);
    expect(info).toEqual(snapshot);
    expect(invoke).toHaveBeenNthCalledWith(1, "start_server", { request });
    expect(invoke).toHaveBeenNthCalledWith(2, "stop_server", undefined);
    expect(invoke).toHaveBeenNthCalledWith(3, "server_status", undefined);
    expect(invoke).toHaveBeenNthCalledWith(4, "current_info_snapshot", undefined);
  });
});
