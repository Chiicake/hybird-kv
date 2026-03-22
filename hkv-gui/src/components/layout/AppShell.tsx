import { Outlet } from "react-router-dom";

import { Sidebar } from "./Sidebar";
import { TopStatusBar } from "./TopStatusBar";

export function AppShell() {
  return (
    <div className="app-shell">
      <div className="app-shell__noise" aria-hidden="true" />
      <Sidebar />
      <div className="app-shell__main">
        <TopStatusBar />
        <main className="app-shell__content" aria-label="Workbench content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
