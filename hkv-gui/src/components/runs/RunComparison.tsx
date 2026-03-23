import type { BenchmarkRun } from "../../lib/types";

type RunComparisonProps = {
  runs: BenchmarkRun[];
};

function formatSigned(value: number, suffix: string) {
  const normalized = Math.round(value * 10) / 10;
  return `${normalized >= 0 ? "+" : ""}${normalized} ${suffix}`;
}

export function RunComparison({ runs }: RunComparisonProps) {
  if (runs.length !== 2) {
    return (
      <article className="panel panel--stacked runs-panel">
        <p className="panel__label">Comparison</p>
        <h2>Two-run compare</h2>
        <p>Select exactly two runs to compare throughput and latency deltas.</p>
        <p>Selected {runs.length} of 2 runs.</p>
      </article>
    );
  }

  const [left, right] = runs;
  const throughputDelta =
    (left.result?.throughputOpsPerSec ?? 0) - (right.result?.throughputOpsPerSec ?? 0);
  const p95Delta = (left.result?.p95LatencyMs ?? 0) - (right.result?.p95LatencyMs ?? 0);

  return (
    <article className="panel panel--stacked runs-panel">
      <p className="panel__label">Comparison</p>
      <h2>{left.id} vs {right.id}</h2>
      <p>Selected 2 of 2 runs.</p>

      <div className="metric-grid runs-metric-grid">
        <div className="metric-card">
          <p className="metric-card__label">Throughput delta</p>
          <strong>{formatSigned(throughputDelta, "ops/s")}</strong>
        </div>
        <div className="metric-card">
          <p className="metric-card__label">P95 latency delta</p>
          <strong>{formatSigned(p95Delta, "ms")}</strong>
        </div>
        <div className="metric-card">
          <p className="metric-card__label">Compared target</p>
          <strong>{left.request.targetAddr}</strong>
        </div>
      </div>
    </article>
  );
}
