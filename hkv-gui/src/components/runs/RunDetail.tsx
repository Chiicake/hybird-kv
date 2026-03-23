import type { BenchmarkRun } from "../../lib/types";

type RunDetailProps = {
  run: BenchmarkRun | null;
};

function detailValue(value: number | null | undefined, suffix: string) {
  return value === null || value === undefined ? "-" : `${value} ${suffix}`;
}

export function RunDetail({ run }: RunDetailProps) {
  return (
    <article className="panel runs-panel">
      <p className="panel__label">Run detail</p>
      <h2>{run ? run.id : "Choose a run"}</h2>
      <p>
        {run
          ? "Single-run metrics and request parameters from persisted history."
          : "Select a run from history to inspect its request, timings, and result metrics."}
      </p>

      {run ? (
        <>
          <dl className="server-kv-list runs-detail-grid">
            <div>
              <dt>Status</dt>
              <dd>{run.status}</dd>
            </div>
            <div>
              <dt>Target</dt>
              <dd>{run.request.targetAddr}</dd>
            </div>
            <div>
              <dt>Clients</dt>
              <dd>{run.request.clients}</dd>
            </div>
            <div>
              <dt>Requests</dt>
              <dd>{run.request.requests}</dd>
            </div>
            <div>
              <dt>Data size</dt>
              <dd>{run.request.dataSize} bytes</dd>
            </div>
            <div>
              <dt>Pipeline</dt>
              <dd>{run.request.pipeline}</dd>
            </div>
          </dl>

          <div className="metric-grid runs-metric-grid">
            <div className="metric-card">
              <p className="metric-card__label">Throughput</p>
              <strong>{detailValue(run.result?.throughputOpsPerSec, "ops/s")}</strong>
            </div>
            <div className="metric-card">
              <p className="metric-card__label">Average latency</p>
              <strong>{detailValue(run.result?.averageLatencyMs, "ms")}</strong>
            </div>
            <div className="metric-card">
              <p className="metric-card__label">P95 latency</p>
              <strong>{detailValue(run.result?.p95LatencyMs, "ms")}</strong>
            </div>
          </div>

          {run.errorMessage ? <p className="server-error">{run.errorMessage}</p> : null}
        </>
      ) : null}
    </article>
  );
}
