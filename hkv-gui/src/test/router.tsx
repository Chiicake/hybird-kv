import { RouterProvider, createMemoryRouter } from "react-router-dom";

import { routes } from "../App";

export function renderTestRouter(path: string) {
  const router = createMemoryRouter(routes, {
    initialEntries: [path],
    future: {
      v7_startTransition: true
    }
  });

  return <RouterProvider router={router} />;
}
