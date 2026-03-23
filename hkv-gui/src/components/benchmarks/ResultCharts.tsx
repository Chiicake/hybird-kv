import { formatLatencyMs, formatOpsPerSec } from "../../lib/format";
import type { BenchmarkRun } from "../../lib/types";

type ResultChartsProps = {
  run: BenchmarkRun | null;
};

function barWidth(value: number, max: number) {
  if (max <= 0) {
    return "0%";
  }

  return `${Math.max(12, Math.round((value / max) * 100))}%`;
}

export function ResultCharts({ run }: ResultChartsProps) {
  const result = run?.result;

  if (!result) {
    return (
      <article className="panel panel--stacked benchmark-panel">
        <p className="panel__label">Charts</p>
        <h2>Throughput and latency sketches</h2>
        <p>Charts appear once the backend returns a completed benchmark result.</p>
      </article>
    );
  }

  const latencySeries = [
    ["P50", result.p50LatencyMs],
    ["P95", result.p95LatencyMs],
    ["P99", result.p99LatencyMs]
  ] as const;
  const latencyMax = Math.max(...latencySeries.map(([, value]) => value));

  return (
    <article className="panel panel--stacked benchmark-panel">
      <p className="panel__label">Charts</p>
      <h2>Throughput and latency sketches</h2>
      <p>Small v1 charts keep the workbench readable without pretending to be a full dashboard.</p>

      <div className="benchmark-chart-grid">
        <figure className="benchmark-chart" role="img" aria-label="Throughput trend">
          <figcaption>Throughput trend</figcaption>
          <div className="benchmark-chart__bar benchmark-chart__bar--throughput">
            <span style={{ width: barWidth(result.throughputOpsPerSec, result.throughputOpsPerSec) }} />
          </div>
          <strong>{formatOpsPerSec(result.throughputOpsPerSec)}</strong>
        </figure>

        <figure className="benchmark-chart" role="img" aria-label="Latency distribution">
          <figcaption>Latency distribution</figcaption>
          <div className="benchmark-chart__series">
            {latencySeries.map(([label, value]) => (
              <div key={label} className="benchmark-chart__row">
                <span>{label}</span>
                <div className="benchmark-chart__bar">
                  <span style={{ width: barWidth(value, latencyMax) }} />
                </div>
                <strong>{formatLatencyMs(value)}</strong>
              </div>
            ))}
          </div>
        </figure>
      </div>
    </article>
  );
}
