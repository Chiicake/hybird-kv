import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { primaryNavItems } from "../routes/route-config";
import { renderTestRouter } from "../test/router";

function renderRoute(path: string) {
  render(renderTestRouter(path));
}

describe("app shell", () => {
  it("renders the navigation links from shared route metadata", () => {
    renderRoute("/");

    expect(
      screen.getByRole("navigation", { name: /primary/i })
    ).toBeInTheDocument();

    primaryNavItems.forEach((item) => {
      expect(screen.getByRole("link", { name: item.label })).toBeInTheDocument();
    });
  });

  it("redirects unknown routes back to overview", () => {
    renderRoute("/does-not-exist");

    expect(
      screen.getByRole("heading", { name: "Overview", level: 1 })
    ).toBeInTheDocument();
  });

  it("renders distinct real-route content for benchmarks and server pages", () => {
    renderRoute("/benchmarks");

    expect(
      screen.getByRole("heading", { name: "Benchmarks", level: 1 })
    ).toBeInTheDocument();
    expect(screen.getByText(/run a focused workbench profile/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start benchmark/i })).toBeInTheDocument();
  });

  it("keeps future pages honest about missing backend dependencies", () => {
    renderRoute("/hot-keys");

    expect(
      screen.getByRole("heading", { name: "Hot Keys", level: 1 })
    ).toBeInTheDocument();
    expect(screen.getByText(/planned expansion/i)).toBeInTheDocument();
    expect(
      screen.getByText(/depends on a future tauri global-shortcut bridge/i)
    ).toBeInTheDocument();
  });

  it("renders distinct real-route content for the server page", () => {
    renderRoute("/server");

    expect(
      screen.getByRole("heading", { name: "Server", level: 1 })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /start local server/i })
    ).toBeInTheDocument();
    expect(screen.getByText("Host")).toBeInTheDocument();
    expect(screen.getByText("127.0.0.1")).toBeInTheDocument();
    expect(screen.getByText("Port")).toBeInTheDocument();
    expect(screen.getByText("6380")).toBeInTheDocument();
  });
});
