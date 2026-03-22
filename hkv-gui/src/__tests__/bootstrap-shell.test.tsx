import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";

import { routes } from "../App";

describe("bootstrap shell", () => {
  it("renders the persistent shell chrome around routed content", () => {
    const router = createMemoryRouter(routes, {
      initialEntries: ["/"]
    });

    render(<RouterProvider router={router} />);

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
