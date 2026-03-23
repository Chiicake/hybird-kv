import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderTestRouter } from "../test/router";

describe("bootstrap shell", () => {
  it("renders the persistent shell chrome around routed content", () => {
    render(renderTestRouter("/"));

    expect(
      screen.getByRole("heading", { name: "HybridKV Workbench" })
    ).toBeInTheDocument();
    expect(
      screen.getByText(/shell online \/ backend pending/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole("main", { name: "Workbench content" })
    ).toBeInTheDocument();
  });
});
