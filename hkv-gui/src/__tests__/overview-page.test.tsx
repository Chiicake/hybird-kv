import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { renderTestRouter } from "../test/router";

vi.mock("../lib/api", () => ({
  loadWorkbenchSnapshot: vi.fn(),
  onBenchmarkEvent: vi.fn(),
  onServerEvent: vi.fn()
}));

describe("overview page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows recent server state, latest run summary, and quick actions", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.loadWorkbenchSnapshot).mockResolvedValue({
      status: {
        state: "running",
        address: "127.0.0.1:6380",
        pid: 4242,
        startedAt: "2026-03-23T12:00:00Z",
        lastError: null
      },
      info: {
        capturedAt: "2026-03-23T12:00:05Z",
        role: "master",
        connectedClients: 3,
        usedMemory: 8192,
        totalCommandsProcessed: 1440,
        instantaneousOpsPerSec: 72,
        keyspaceHits: 100,
        keyspaceMisses: 8,
        uptimeSeconds: 95
      },
      latestRun: {
        id: "run-002",
        runner: "redis-benchmark",
        status: "completed",
        targetAddr: "127.0.0.1:6380",
        createdAt: "2026-03-23T12:05:00Z",
        finishedAt: "2026-03-23T12:05:04Z",
        throughputOpsPerSec: 180000,
        p95LatencyMs: 1.4
      },
      previousRun: {
        id: "run-001",
        runner: "redis-benchmark",
        status: "completed",
        targetAddr: "127.0.0.1:6380",
        createdAt: "2026-03-23T11:59:00Z",
        finishedAt: "2026-03-23T11:59:04Z",
        throughputOpsPerSec: 150000,
        p95LatencyMs: 2.1
      },
      recentRuns: [
        {
          id: "run-002",
          runner: "redis-benchmark",
          status: "completed",
          targetAddr: "127.0.0.1:6380",
          createdAt: "2026-03-23T12:05:00Z",
          finishedAt: "2026-03-23T12:05:04Z",
          throughputOpsPerSec: 180000,
          p95LatencyMs: 1.4
        },
        {
          id: "run-001",
          runner: "redis-benchmark",
          status: "completed",
          targetAddr: "127.0.0.1:6380",
          createdAt: "2026-03-23T11:59:00Z",
          finishedAt: "2026-03-23T11:59:04Z",
          throughputOpsPerSec: 150000,
          p95LatencyMs: 2.1
        }
      ]
    });

    render(renderTestRouter("/"));

    expect(await screen.findByRole("heading", { name: "Overview", level: 1 })).toBeInTheDocument();
    expect(screen.getByText(/server state/i)).toBeInTheDocument();
    expect(screen.getAllByText("running").length).toBeGreaterThan(0);
    expect(screen.getByText("127.0.0.1:6380")).toBeInTheDocument();
    expect(screen.getByText("180000 ops/s")).toBeInTheDocument();
    expect(screen.getByText("1.4 ms")).toBeInTheDocument();
    expect(screen.getByText("+30000 ops/s")).toBeInTheDocument();
    expect(screen.getByText("-0.7 ms p95")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /open benchmarks/i })).toHaveAttribute("href", "/benchmarks");
    expect(screen.getByRole("link", { name: /open server controls/i })).toHaveAttribute("href", "/server");

    await waitFor(() => {
      expect(api.loadWorkbenchSnapshot).toHaveBeenCalledTimes(1);
    });

    expect(screen.getAllByText(/server running at 127.0.0.1:6380/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/latest run run-002 completed/i).length).toBeGreaterThan(0);
  });

  it("keeps the shell honest when no server or runs are active", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.loadWorkbenchSnapshot).mockResolvedValue({
      status: {
        state: "stopped",
        address: "127.0.0.1:6380",
        pid: null,
        startedAt: null,
        lastError: null
      },
      info: null,
      latestRun: null,
      previousRun: null,
      recentRuns: []
    });

    render(renderTestRouter("/"));

    expect((await screen.findAllByText(/server is stopped/i)).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/no benchmark run recorded yet/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/launch your first benchmark from the workbench/i)).toBeInTheDocument();
  });

  it("refreshes the shared workbench snapshot when benchmark or server events arrive", async () => {
    const api = await import("../lib/api");
    let benchmarkHandler: (() => void) | null = null;
    let serverHandler: (() => void) | null = null;

    vi.mocked(api.onBenchmarkEvent).mockImplementation(async (handler) => {
      benchmarkHandler = () =>
        handler({
          channel: "benchmark:lifecycle",
          event: "completed",
          runId: "run-002",
          emittedAt: "2026-03-23T12:06:00Z",
          message: "done",
          error: null
        });
      return () => undefined;
    });

    vi.mocked(api.onServerEvent).mockImplementation(async (handler) => {
      serverHandler = () =>
        handler({
          channel: "server:status",
          event: "state-changed",
          emittedAt: "2026-03-23T12:06:01Z",
          status: {
            state: "running",
            address: "127.0.0.1:6380",
            pid: 4242,
            startedAt: "2026-03-23T12:00:00Z",
            lastError: null
          },
          info: null
        });
      return () => undefined;
    });

    vi.mocked(api.loadWorkbenchSnapshot)
      .mockResolvedValueOnce({
        status: {
          state: "stopped",
          address: "127.0.0.1:6380",
          pid: null,
          startedAt: null,
          lastError: null
        },
        info: null,
        latestRun: null,
        previousRun: null,
        recentRuns: []
      })
      .mockResolvedValue({
        status: {
          state: "running",
          address: "127.0.0.1:6380",
          pid: 4242,
          startedAt: "2026-03-23T12:00:00Z",
          lastError: null
        },
        info: null,
        latestRun: {
          id: "run-002",
          runner: "redis-benchmark",
          status: "completed",
          targetAddr: "127.0.0.1:6380",
          createdAt: "2026-03-23T12:05:00Z",
          finishedAt: "2026-03-23T12:05:04Z",
          throughputOpsPerSec: 180000,
          p95LatencyMs: 1.4
        },
        previousRun: null,
        recentRuns: []
      });

    render(renderTestRouter("/"));

    await waitFor(() => {
      expect(api.loadWorkbenchSnapshot).toHaveBeenCalledTimes(1);
    });

    await act(async () => {
      benchmarkHandler?.();
      serverHandler?.();
    });

    await waitFor(() => {
      expect(api.loadWorkbenchSnapshot).toHaveBeenCalledTimes(3);
    });
  });

  it("stays stable when the shared snapshot loader fails", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.onBenchmarkEvent).mockResolvedValue(() => undefined);
    vi.mocked(api.onServerEvent).mockResolvedValue(() => undefined);
    vi.mocked(api.loadWorkbenchSnapshot).mockRejectedValue(new Error("snapshot load failed"));

    render(renderTestRouter("/"));

    expect(await screen.findByRole("heading", { name: "Overview", level: 1 })).toBeInTheDocument();
    expect(screen.getAllByText(/loading local server/i).length).toBeGreaterThan(0);
  });
});
