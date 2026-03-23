import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { BENCHMARK_EVENT_CHANNEL, type BenchmarkRun } from "../lib/types";
import { Benchmarks } from "../routes/Benchmarks";
import {
  RUNTIME_PREFERENCES_STORAGE_KEY,
  saveRuntimePreferences
} from "../lib/runtime-preferences";

vi.mock("../lib/api", () => ({
  getRunDetail: vi.fn(),
  onBenchmarkEvent: vi.fn(),
  startBenchmark: vi.fn(),
  stopBenchmark: vi.fn()
}));

describe("benchmarks page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
  });

  it("prefills the benchmark target from saved runtime preferences", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.onBenchmarkEvent).mockResolvedValue(() => undefined);
    saveRuntimePreferences({
      benchmarkBinaryPath: "~/bin/redis-benchmark",
      benchmarkTargetHost: "192.168.1.44",
      benchmarkTargetPort: "6390"
    });

    render(<Benchmarks />);

    expect(screen.getByLabelText(/target host/i)).toHaveValue("192.168.1.44");
    expect(screen.getByLabelText(/target port/i)).toHaveValue("6390");
    expect(window.localStorage.getItem(RUNTIME_PREFERENCES_STORAGE_KEY)).toContain("redis-benchmark");
  });

  it("configures a run, shows live progress, and refreshes detail from lifecycle events", async () => {
    const api = await import("../lib/api");
    const unlisten = vi.fn();
    let benchmarkHandler: ((payload: {
      channel: string;
      event: string;
      runId: string;
      emittedAt: string;
      message: string | null;
      error: string | null;
    }) => void) | null = null;

    vi.mocked(api.onBenchmarkEvent).mockImplementation(async (handler) => {
      benchmarkHandler = handler;
      return unlisten;
    });

    vi.mocked(api.startBenchmark).mockResolvedValue({
      id: "run-9000",
      request: {
        runner: "redis-benchmark",
        targetAddr: "10.0.0.5:6380",
        clients: 48,
        requests: 300000,
        dataSize: 512,
        pipeline: 8
      },
      status: "queued",
      createdAt: "2026-03-23T12:30:00Z",
      startedAt: null,
      finishedAt: null,
      result: null,
      errorMessage: null
    });

    vi.mocked(api.getRunDetail)
      .mockResolvedValueOnce({
        id: "run-9000",
        request: {
          runner: "redis-benchmark",
          targetAddr: "10.0.0.5:6380",
          clients: 48,
          requests: 300000,
          dataSize: 512,
          pipeline: 8
        },
        status: "running",
        createdAt: "2026-03-23T12:30:00Z",
        startedAt: "2026-03-23T12:30:01Z",
        finishedAt: null,
        result: null,
        errorMessage: null
      })
      .mockResolvedValueOnce({
        id: "run-9000",
        request: {
          runner: "redis-benchmark",
          targetAddr: "10.0.0.5:6380",
          clients: 48,
          requests: 300000,
          dataSize: 512,
          pipeline: 8
        },
        status: "completed",
        createdAt: "2026-03-23T12:30:00Z",
        startedAt: "2026-03-23T12:30:01Z",
        finishedAt: "2026-03-23T12:30:12Z",
        result: {
          totalRequests: 300000,
          throughputOpsPerSec: 186500,
          averageLatencyMs: 1.3,
          p50LatencyMs: 0.9,
          p95LatencyMs: 2.4,
          p99LatencyMs: 3.1,
          durationMs: 11000,
          datasetBytes: 153600000
        },
        errorMessage: null
      });

    vi.mocked(api.stopBenchmark).mockResolvedValue({
      id: "run-9000",
      request: {
        runner: "redis-benchmark",
        targetAddr: "10.0.0.5:6380",
        clients: 48,
        requests: 300000,
        dataSize: 512,
        pipeline: 8
      },
      status: "cancelled",
      createdAt: "2026-03-23T12:30:00Z",
      startedAt: "2026-03-23T12:30:01Z",
      finishedAt: "2026-03-23T12:30:12Z",
      result: null,
      errorMessage: null
    });

    const { unmount } = render(<Benchmarks />);

    fireEvent.change(screen.getByLabelText(/target host/i), {
      target: { value: "10.0.0.5" }
    });
    fireEvent.change(screen.getByLabelText(/target port/i), {
      target: { value: "6380" }
    });
    fireEvent.change(screen.getByLabelText(/benchmark profile/i), {
      target: { value: "throughput" }
    });
    fireEvent.change(screen.getByLabelText(/request count/i), {
      target: { value: "300000" }
    });
    fireEvent.change(screen.getByLabelText(/concurrency/i), {
      target: { value: "48" }
    });
    fireEvent.change(screen.getByLabelText(/payload size/i), {
      target: { value: "512" }
    });
    fireEvent.change(screen.getByLabelText(/pipeline depth/i), {
      target: { value: "8" }
    });

    fireEvent.click(screen.getByRole("button", { name: /start benchmark/i }));

    await waitFor(() => {
      expect(api.startBenchmark).toHaveBeenCalledWith({
        runner: "redis-benchmark",
        targetAddr: "10.0.0.5:6380",
        clients: 48,
        requests: 300000,
        dataSize: 512,
        pipeline: 8
      });
    });

    expect(screen.getAllByText("run-9000").length).toBeGreaterThan(0);
    expect(screen.getByText(/queued/i)).toBeInTheDocument();
    expect(screen.getByText(/10.0.0.5:6380/i)).toBeInTheDocument();
    expect(screen.getAllByText(/redis-benchmark/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/waiting for benchmark metrics/i)).toBeInTheDocument();

    await act(async () => {
      benchmarkHandler?.({
        channel: BENCHMARK_EVENT_CHANNEL,
        event: "running",
        runId: "run-9000",
        emittedAt: "2026-03-23T12:30:02Z",
        message: "started redis-benchmark against 10.0.0.5:6380 with 48 clients",
        error: null
      });
    });

    await waitFor(() => {
      expect(api.getRunDetail).toHaveBeenCalledWith("run-9000");
    });

    expect(
      screen.getByText(/started redis-benchmark against 10.0.0.5:6380 with 48 clients/i)
    ).toBeInTheDocument();
    expect(screen.getAllByText(/^running$/i).length).toBeGreaterThan(0);

    await act(async () => {
      benchmarkHandler?.({
        channel: BENCHMARK_EVENT_CHANNEL,
        event: "completed",
        runId: "run-9000",
        emittedAt: "2026-03-23T12:30:12Z",
        message: "benchmark complete",
        error: null
      });
    });

    await waitFor(() => {
      expect(api.getRunDetail).toHaveBeenCalledTimes(2);
    });

    expect(screen.getAllByText("186500 ops/s").length).toBeGreaterThan(0);
    expect(screen.getAllByText("2.4 ms").length).toBeGreaterThan(0);
    expect(screen.getByText("11 s")).toBeInTheDocument();
    expect(screen.getByText("146.5 MiB")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /throughput trend/i })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /latency distribution/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /stop benchmark/i }));

    await waitFor(() => {
      expect(api.stopBenchmark).toHaveBeenCalledWith("run-9000");
    });

    unmount();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("validates required benchmark fields before invoking the backend", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.onBenchmarkEvent).mockResolvedValue(() => undefined);

    render(<Benchmarks />);

    fireEvent.change(screen.getByLabelText(/target host/i), {
      target: { value: "" }
    });
    fireEvent.change(screen.getByLabelText(/request count/i), {
      target: { value: "0" }
    });
    fireEvent.change(screen.getByLabelText(/concurrency/i), {
      target: { value: "0" }
    });

    fireEvent.click(screen.getByRole("button", { name: /start benchmark/i }));

    expect(api.startBenchmark).not.toHaveBeenCalled();
    expect(screen.getByText(/host is required/i)).toBeInTheDocument();
    expect(screen.getByText(/request count must be greater than zero/i)).toBeInTheDocument();
    expect(screen.getByText(/concurrency must be greater than zero/i)).toBeInTheDocument();
  });

  it("supports duration mode as a real v1 form input", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.onBenchmarkEvent).mockResolvedValue(() => undefined);
    vi.mocked(api.startBenchmark).mockResolvedValue({
      id: "run-duration",
      request: {
        runner: "redis-benchmark",
        targetAddr: "127.0.0.1:6379",
        clients: 12,
        requests: 45000,
        dataSize: 128,
        pipeline: 1
      },
      status: "queued",
      createdAt: "2026-03-23T13:00:00Z",
      startedAt: null,
      finishedAt: null,
      result: null,
      errorMessage: null
    });

    render(<Benchmarks />);

    fireEvent.change(screen.getByLabelText(/benchmark mode/i), {
      target: { value: "duration" }
    });
    fireEvent.change(screen.getByLabelText(/duration/i), {
      target: { value: "9" }
    });
    fireEvent.change(screen.getByLabelText(/concurrency/i), {
      target: { value: "12" }
    });
    fireEvent.change(screen.getByLabelText(/pipeline depth/i), {
      target: { value: "1" }
    });

    fireEvent.click(screen.getByRole("button", { name: /start benchmark/i }));

    await waitFor(() => {
      expect(api.startBenchmark).toHaveBeenCalledWith({
        runner: "redis-benchmark",
        targetAddr: "127.0.0.1:6379",
        clients: 12,
        requests: 45000,
        dataSize: 128,
        pipeline: 1
      });
    });

    expect(
      screen.getByText(/maps duration to a request budget before launching the backend runner/i)
    ).toBeInTheDocument();
  });

  it("ignores benchmark events for runs other than the active run", async () => {
    const api = await import("../lib/api");
    let benchmarkHandler: ((payload: {
      channel: string;
      event: string;
      runId: string;
      emittedAt: string;
      message: string | null;
      error: string | null;
    }) => void) | null = null;

    vi.mocked(api.onBenchmarkEvent).mockImplementation(async (handler) => {
      benchmarkHandler = handler;
      return () => undefined;
    });
    vi.mocked(api.startBenchmark).mockResolvedValue({
      id: "run-111",
      request: {
        runner: "redis-benchmark",
        targetAddr: "127.0.0.1:6379",
        clients: 16,
        requests: 100000,
        dataSize: 128,
        pipeline: 2
      },
      status: "queued",
      createdAt: "2026-03-23T13:10:00Z",
      startedAt: null,
      finishedAt: null,
      result: null,
      errorMessage: null
    });

    render(<Benchmarks />);

    fireEvent.click(screen.getByRole("button", { name: /start benchmark/i }));

    await waitFor(() => {
      expect(api.startBenchmark).toHaveBeenCalled();
    });

    await act(async () => {
      benchmarkHandler?.({
        channel: BENCHMARK_EVENT_CHANNEL,
        event: "running",
        runId: "run-other",
        emittedAt: "2026-03-23T13:10:01Z",
        message: "other run progress",
        error: null
      });
    });

    expect(api.getRunDetail).not.toHaveBeenCalled();
    expect(screen.queryByText(/other run progress/i)).not.toBeInTheDocument();
  });

  it("surfaces refresh errors when lifecycle detail reload fails", async () => {
    const api = await import("../lib/api");
    let benchmarkHandler: ((payload: {
      channel: string;
      event: string;
      runId: string;
      emittedAt: string;
      message: string | null;
      error: string | null;
    }) => void) | null = null;

    vi.mocked(api.onBenchmarkEvent).mockImplementation(async (handler) => {
      benchmarkHandler = handler;
      return () => undefined;
    });
    vi.mocked(api.startBenchmark).mockResolvedValue({
      id: "run-error",
      request: {
        runner: "redis-benchmark",
        targetAddr: "127.0.0.1:6379",
        clients: 16,
        requests: 100000,
        dataSize: 128,
        pipeline: 2
      },
      status: "queued",
      createdAt: "2026-03-23T13:20:00Z",
      startedAt: null,
      finishedAt: null,
      result: null,
      errorMessage: null
    });
    vi.mocked(api.getRunDetail).mockRejectedValue(new Error("detail reload failed"));

    render(<Benchmarks />);

    fireEvent.click(screen.getByRole("button", { name: /start benchmark/i }));

    await waitFor(() => {
      expect(api.startBenchmark).toHaveBeenCalled();
    });

    await act(async () => {
      benchmarkHandler?.({
        channel: BENCHMARK_EVENT_CHANNEL,
        event: "running",
        runId: "run-error",
        emittedAt: "2026-03-23T13:20:01Z",
        message: "refresh me",
        error: null
      });
    });

    await waitFor(() => {
      expect(api.getRunDetail).toHaveBeenCalledWith("run-error");
    });

    expect(screen.getByText(/detail reload failed/i)).toBeInTheDocument();
  });

  it("queues a follow-up refresh when benchmark events arrive during an in-flight detail load", async () => {
    const api = await import("../lib/api");
    let benchmarkHandler: ((payload: {
      channel: string;
      event: string;
      runId: string;
      emittedAt: string;
      message: string | null;
      error: string | null;
    }) => void) | null = null;

    vi.mocked(api.onBenchmarkEvent).mockImplementation(async (handler) => {
      benchmarkHandler = handler;
      return () => undefined;
    });
    vi.mocked(api.startBenchmark).mockResolvedValue({
      id: "run-coalesce",
      request: {
        runner: "redis-benchmark",
        targetAddr: "127.0.0.1:6379",
        clients: 16,
        requests: 100000,
        dataSize: 128,
        pipeline: 2
      },
      status: "queued",
      createdAt: "2026-03-23T13:30:00Z",
      startedAt: null,
      finishedAt: null,
      result: null,
      errorMessage: null
    });

    let resolveFirst: ((value: BenchmarkRun) => void) | null = null;
    const firstDetail = new Promise<BenchmarkRun>((resolve) => {
      resolveFirst = resolve;
    });

    vi.mocked(api.getRunDetail)
      .mockImplementationOnce(() => firstDetail)
      .mockResolvedValueOnce({
        id: "run-coalesce",
        request: {
          runner: "redis-benchmark",
          targetAddr: "127.0.0.1:6379",
          clients: 16,
          requests: 100000,
          dataSize: 128,
          pipeline: 2
        },
        status: "completed",
        createdAt: "2026-03-23T13:30:00Z",
        startedAt: "2026-03-23T13:30:01Z",
        finishedAt: "2026-03-23T13:30:08Z",
        result: {
          totalRequests: 100000,
          throughputOpsPerSec: 99000,
          averageLatencyMs: 1.0,
          p50LatencyMs: 0.7,
          p95LatencyMs: 1.8,
          p99LatencyMs: 2.6,
          durationMs: 8000,
          datasetBytes: 12800000
        },
        errorMessage: null
      });

    render(<Benchmarks />);

    fireEvent.click(screen.getByRole("button", { name: /start benchmark/i }));

    await waitFor(() => {
      expect(api.startBenchmark).toHaveBeenCalled();
    });

    await act(async () => {
      benchmarkHandler?.({
        channel: BENCHMARK_EVENT_CHANNEL,
        event: "running",
        runId: "run-coalesce",
        emittedAt: "2026-03-23T13:30:01Z",
        message: "first refresh",
        error: null
      });
      benchmarkHandler?.({
        channel: BENCHMARK_EVENT_CHANNEL,
        event: "completed",
        runId: "run-coalesce",
        emittedAt: "2026-03-23T13:30:08Z",
        message: "needs a second refresh",
        error: null
      });
    });

    resolveFirst?.({
      id: "run-coalesce",
      request: {
        runner: "redis-benchmark",
        targetAddr: "127.0.0.1:6379",
        clients: 16,
        requests: 100000,
        dataSize: 128,
        pipeline: 2
      },
      status: "running",
      createdAt: "2026-03-23T13:30:00Z",
      startedAt: "2026-03-23T13:30:01Z",
      finishedAt: null,
      result: null,
      errorMessage: null
    });

    await waitFor(() => {
      expect(api.getRunDetail).toHaveBeenCalledTimes(2);
    });

    expect(screen.getAllByText("99000 ops/s").length).toBeGreaterThan(0);
  });
});
