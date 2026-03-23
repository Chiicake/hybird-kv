import { Outlet } from "react-router-dom";

import { Sidebar } from "./Sidebar";
import { TopStatusBar } from "./TopStatusBar";
import { useWorkbenchSnapshot } from "./workbenchSnapshot";

export function AppShell() {
  const snapshot = useWorkbenchSnapshot();

  return (
    <div className="app-shell">
      <div className="app-shell__noise" aria-hidden="true" />
      <Sidebar />
      <div className="app-shell__main">
        <TopStatusBar snapshot={snapshot} />
        <main className="app-shell__content" aria-label="Workbench content">
          <Outlet context={snapshot} />
        </main>
      </div>
    </div>
  );
}
