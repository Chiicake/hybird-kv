import type { WorkbenchSnapshot } from "../../lib/api";
import { summarizeLatestRun, summarizeWorkbenchStatus } from "./workbenchSnapshot";

type TopStatusBarProps = {
  snapshot: WorkbenchSnapshot | null;
};

export function TopStatusBar({ snapshot }: TopStatusBarProps) {
  const statusChipClass = snapshot?.status.state === "running" ? "status-chip status-chip--ok" : "status-chip";

  return (
    <header className="top-status-bar">
      <div>
        <p className="top-status-bar__label">Workbench state</p>
        <p className="top-status-bar__value">{summarizeWorkbenchStatus(snapshot)}</p>
      </div>

      <div className="top-status-bar__chips" aria-label="Status indicators">
        <span className={statusChipClass}>{snapshot?.status.state ?? "loading"}</span>
        <span className="status-chip">{summarizeLatestRun(snapshot)}</span>
        <span className="status-chip">
          {snapshot?.info ? `${snapshot.info.instantaneousOpsPerSec} ops/s live` : "No live info sample"}
        </span>
      </div>
    </header>
  );
}
