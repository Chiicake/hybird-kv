import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";

import { Server } from "../routes/Server";

vi.mock("../lib/api", () => ({
  currentInfoSnapshot: vi.fn(),
  serverStatus: vi.fn(),
  startServer: vi.fn(),
  stopServer: vi.fn()
}));

describe("server page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("updates status and info after starting the local server", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.startServer).mockResolvedValue({
      state: "running",
      address: "127.0.0.1:6380",
      pid: 4242,
      startedAt: "2026-03-23T12:00:00Z",
      lastError: null
    });
    vi.mocked(api.currentInfoSnapshot).mockResolvedValue({
      capturedAt: "2026-03-23T12:00:01Z",
      role: "master",
      connectedClients: 2,
      usedMemory: 4096,
      totalCommandsProcessed: 88,
      instantaneousOpsPerSec: 21,
      keyspaceHits: 5,
      keyspaceMisses: 1,
      uptimeSeconds: 34
    });

    render(<Server />);

    fireEvent.click(screen.getByRole("button", { name: /start local server/i }));

    await waitFor(() => {
      expect(screen.getByText("running")).toBeInTheDocument();
      expect(screen.getByText("4242")).toBeInTheDocument();
      expect(screen.getByText("88")).toBeInTheDocument();
      expect(screen.getByText("21")).toBeInTheDocument();
    });
  });

  it("clears the info summary after stopping the server", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.startServer).mockResolvedValue({
      state: "running",
      address: "127.0.0.1:6380",
      pid: 4242,
      startedAt: "2026-03-23T12:00:00Z",
      lastError: null
    });
    vi.mocked(api.currentInfoSnapshot).mockResolvedValue({
      capturedAt: "2026-03-23T12:00:01Z",
      role: "master",
      connectedClients: 2,
      usedMemory: 4096,
      totalCommandsProcessed: 88,
      instantaneousOpsPerSec: 21,
      keyspaceHits: 5,
      keyspaceMisses: 1,
      uptimeSeconds: 34
    });
    vi.mocked(api.stopServer).mockResolvedValue({
      state: "stopped",
      address: "127.0.0.1:6380",
      pid: null,
      startedAt: null,
      lastError: null
    });

    render(<Server />);

    fireEvent.click(screen.getByRole("button", { name: /start local server/i }));
    await screen.findByText("running");

    fireEvent.click(screen.getByRole("button", { name: /stop server/i }));

    await waitFor(() => {
      expect(screen.getByText("stopped")).toBeInTheDocument();
      expect(screen.getByText(/waiting for a running server/i)).toBeInTheDocument();
    });
  });

  it("surfaces start failures through the existing last error UI", async () => {
    const api = await import("../lib/api");

    vi.mocked(api.startServer).mockRejectedValue(new Error("binary unavailable"));

    render(<Server />);

    fireEvent.click(screen.getByRole("button", { name: /start local server/i }));

    await waitFor(() => {
      expect(screen.getByText("binary unavailable")).toBeInTheDocument();
      expect(screen.getByText("stopped")).toBeInTheDocument();
    });
  });
});
