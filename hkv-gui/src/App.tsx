import { Navigate, type RouteObject } from "react-router-dom";

import { AppShell } from "./components/layout/AppShell";
import { primaryRouteChildren } from "./routes/config";
import "./styles/index.css";

export const routes: RouteObject[] = [
  {
    path: "/",
    element: <AppShell />,
    children: [
      ...primaryRouteChildren,
      { path: "*", element: <Navigate to="/" replace /> }
    ]
  }
];
