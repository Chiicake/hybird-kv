import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { Settings } from "../routes/Settings";
import { DEFAULT_RUNTIME_PREFERENCES, loadRuntimePreferences, saveRuntimePreferences } from "../lib/runtime-preferences";

describe("settings page", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("starts with only the minimal v1 runtime preferences", () => {
    render(<Settings />);

    expect(screen.getByRole("heading", { name: "Settings", level: 1 })).toBeInTheDocument();
    expect(screen.getByLabelText(/benchmark executable path override/i)).toHaveValue(
      DEFAULT_RUNTIME_PREFERENCES.benchmarkBinaryPath
    );
    expect(screen.getByLabelText(/default benchmark target host/i)).toHaveValue("127.0.0.1");
    expect(screen.getByLabelText(/default benchmark target port/i)).toHaveValue("6379");
    expect(screen.getByText(/nothing else is configurable in v1/i)).toBeInTheDocument();
  });

  it("persists the v1 runtime preferences locally", () => {
    render(<Settings />);

    fireEvent.change(screen.getByLabelText(/benchmark executable path override/i), {
      target: { value: "$HOME/bin/redis-benchmark" }
    });
    fireEvent.change(screen.getByLabelText(/default benchmark target host/i), {
      target: { value: "10.1.0.9" }
    });
    fireEvent.change(screen.getByLabelText(/default benchmark target port/i), {
      target: { value: "6388" }
    });
    fireEvent.click(screen.getByRole("button", { name: /save runtime preferences/i }));

    expect(loadRuntimePreferences()).toEqual({
      benchmarkBinaryPath: "$HOME/bin/redis-benchmark",
      benchmarkTargetHost: "10.1.0.9",
      benchmarkTargetPort: "6388"
    });
    expect(screen.getByText(/saved in local browser-backed app storage/i)).toBeInTheDocument();
  });

  it("shows placeholder expansion previews without pretending they already work", () => {
    saveRuntimePreferences({
      benchmarkBinaryPath: "$HOME/tools/redis-benchmark",
      benchmarkTargetHost: "127.0.0.1",
      benchmarkTargetPort: "6379"
    });
    render(<Settings />);

    expect(loadRuntimePreferences().benchmarkBinaryPath).toBe("$HOME/tools/redis-benchmark");
    expect(screen.getByText(/expansion preview for a future backend adapter/i)).toBeInTheDocument();
    expect(screen.getByText(/actual path resolution depends on a later tauri backend capability/i)).toBeInTheDocument();
    expect(screen.getByText(/with home expanded later/i)).toBeInTheDocument();
  });
});
