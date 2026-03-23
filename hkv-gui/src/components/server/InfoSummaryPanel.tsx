import type { InfoSnapshot } from "../../lib/types";

type InfoSummaryPanelProps = {
  snapshot: InfoSnapshot | null;
};

const emptyMetrics = [
  ["Role", "-"],
  ["Commands", "-"],
  ["Ops/sec", "-"],
  ["Uptime", "-"],
  ["Clients", "-"],
  ["Memory", "- bytes"]
] as const;

export function InfoSummaryPanel({ snapshot }: InfoSummaryPanelProps) {
  const metrics = snapshot
    ? [
        ["Role", snapshot.role],
        ["Commands", snapshot.totalCommandsProcessed.toLocaleString()],
        ["Ops/sec", snapshot.instantaneousOpsPerSec.toLocaleString()],
        ["Uptime", `${snapshot.uptimeSeconds}s`],
        ["Clients", snapshot.connectedClients.toLocaleString()],
        ["Memory", `${snapshot.usedMemory.toLocaleString()} bytes`]
      ]
    : emptyMetrics;

  return (
    <article className="panel panel--stacked server-panel">
      <p className="panel__label">INFO summary</p>
      <h2>Compact live metrics</h2>
      <p>
        Normalized from the server `INFO` response on each status refresh.
      </p>

      <div className="metric-grid server-metric-grid">
        {metrics.map(([label, value]) => (
          <div key={label} className="metric-card">
            <p className="metric-card__label">{label}</p>
            <strong>{value}</strong>
          </div>
        ))}
      </div>

      <p className="server-captured-at">
        Captured: {snapshot?.capturedAt ?? "waiting for a running server"}
      </p>
    </article>
  );
}
