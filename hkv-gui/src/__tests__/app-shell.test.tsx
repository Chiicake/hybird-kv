import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";

import { routes } from "../App";
import { primaryNavItems } from "../routes/config";

function renderRoute(path: string) {
  const router = createMemoryRouter(routes, {
    initialEntries: [path]
  });

  render(<RouterProvider router={router} />);
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
    expect(
      screen.getByText(/navigation is live; execution is intentionally absent/i)
    ).toBeInTheDocument();
  });

  it("keeps future pages honest about missing backend dependencies", () => {
    renderRoute("/hot-keys");

    expect(
      screen.getByRole("heading", { name: "Hot Keys", level: 1 })
    ).toBeInTheDocument();
    expect(screen.getByText(/planned expansion/i)).toBeInTheDocument();
    expect(
      screen.getByText(/depends on tauri command bindings and runtime services/i)
    ).toBeInTheDocument();
  });

  it("renders distinct real-route content for the server page", () => {
    renderRoute("/server");

    expect(
      screen.getByRole("heading", { name: "Server", level: 1 })
    ).toBeInTheDocument();
    expect(
      screen.getByText(/no fake toggles before process management exists/i)
    ).toBeInTheDocument();
  });
});
