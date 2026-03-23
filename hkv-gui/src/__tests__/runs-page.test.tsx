import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Runs } from "../routes/Runs";

vi.mock("../lib/api", () => ({
  getRunDetail: vi.fn(),
  listRuns: vi.fn()
}));

describe("runs page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("loads persisted history, shows selected detail, and enables two-run comparison", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.listRuns).mockResolvedValue([
      {
        id: "run-002",
        runner: "redis-benchmark",
        status: "completed",
        targetAddr: "127.0.0.1:6379",
        createdAt: "2026-03-23T12:05:00Z",
        finishedAt: "2026-03-23T12:05:05Z",
        throughputOpsPerSec: 150000,
        p95LatencyMs: 1.7
      },
      {
        id: "run-001",
        runner: "redis-benchmark",
        status: "completed",
        targetAddr: "127.0.0.1:6379",
        createdAt: "2026-03-23T12:00:00Z",
        finishedAt: "2026-03-23T12:00:05Z",
        throughputOpsPerSec: 120000,
        p95LatencyMs: 2.3
      }
    ]);
    vi.mocked(api.getRunDetail)
      .mockResolvedValueOnce({
        id: "run-002",
        request: {
          runner: "redis-benchmark",
          targetAddr: "127.0.0.1:6379",
          clients: 64,
          requests: 200000,
          dataSize: 128,
          pipeline: 4
        },
        status: "completed",
        createdAt: "2026-03-23T12:05:00Z",
        startedAt: "2026-03-23T12:05:01Z",
        finishedAt: "2026-03-23T12:05:05Z",
        result: {
          totalRequests: 200000,
          throughputOpsPerSec: 150000,
          averageLatencyMs: 1.1,
          p50LatencyMs: 0.8,
          p95LatencyMs: 1.7,
          p99LatencyMs: 2.2,
          durationMs: 4000,
          datasetBytes: 25600000
        },
        errorMessage: null
      })
      .mockResolvedValueOnce({
        id: "run-001",
        request: {
          runner: "redis-benchmark",
          targetAddr: "127.0.0.1:6379",
          clients: 32,
          requests: 100000,
          dataSize: 128,
          pipeline: 2
        },
        status: "completed",
        createdAt: "2026-03-23T12:00:00Z",
        startedAt: "2026-03-23T12:00:01Z",
        finishedAt: "2026-03-23T12:00:05Z",
        result: {
          totalRequests: 100000,
          throughputOpsPerSec: 120000,
          averageLatencyMs: 1.4,
          p50LatencyMs: 0.9,
          p95LatencyMs: 2.3,
          p99LatencyMs: 3.1,
          durationMs: 4200,
          datasetBytes: 12800000
        },
        errorMessage: null
      });

    render(<Runs />);

    expect(await screen.findByText("run-002")).toBeInTheDocument();
    expect(
      screen.getByRole("list", { name: /persisted benchmark timeline/i })
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(api.getRunDetail).toHaveBeenCalledWith("run-002");
    });

    expect(screen.getByText("Run detail")).toBeInTheDocument();
    expect(screen.getAllByText("150000 ops/s")).toHaveLength(2);

    fireEvent.click(screen.getByLabelText("Compare run-002"));
    fireEvent.click(screen.getByRole("button", { name: /select run run-001/i }));

    await waitFor(() => {
      expect(api.getRunDetail).toHaveBeenCalledWith("run-001");
    });

    expect(screen.getByText(/selected 2 of 2 runs/i)).toBeInTheDocument();
    expect(screen.getByText("+30000 ops/s")).toBeInTheDocument();
    expect(screen.getByText("-0.6 ms")).toBeInTheDocument();
  });

  it("shows an empty state when no runs have been persisted yet", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.listRuns).mockResolvedValue([]);

    render(<Runs />);

    expect(await screen.findByText(/no persisted benchmark runs yet/i)).toBeInTheDocument();
  });
});
