import {
  formatBytes,
  formatDurationSeconds,
  formatLatencyMs,
  formatOpsPerSec
} from "../../lib/format";
import type { BenchmarkRun } from "../../lib/types";

type MetricsCardsProps = {
  run: BenchmarkRun | null;
};

export function MetricsCards({ run }: MetricsCardsProps) {
  const result = run?.result;
  const waiting = !result;

  const metrics = [
    ["Throughput", formatOpsPerSec(result?.throughputOpsPerSec)],
    ["Average latency", formatLatencyMs(result?.averageLatencyMs)],
    ["P95 latency", formatLatencyMs(result?.p95LatencyMs)],
    ["Duration", formatDurationSeconds(result?.durationMs)],
    ["Dataset", formatBytes(result?.datasetBytes)],
    ["Total requests", result ? String(result.totalRequests) : "-"]
  ] as const;

  return (
    <article className="panel panel--stacked benchmark-panel">
      <p className="panel__label">Live metrics</p>
      <h2>Key benchmark readouts</h2>
      <p>{waiting ? "Waiting for benchmark metrics." : "Latest summary from the active run."}</p>

      <div className="metric-grid benchmark-metric-grid">
        {metrics.map(([label, value]) => (
          <div key={label} className="metric-card">
            <p className="metric-card__label">{label}</p>
            <strong>{value}</strong>
          </div>
        ))}
      </div>
    </article>
  );
}
