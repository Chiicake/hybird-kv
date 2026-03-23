import { Link, useOutletContext } from "react-router-dom";

import type { WorkbenchSnapshot } from "../lib/api";
import { summarizeLatestRun, summarizeWorkbenchStatus } from "../components/layout/workbenchSnapshot";
import { formatLatencyMs, formatOpsPerSec } from "../lib/format";

function deltaLabel(current: number | null, previous: number | null, suffix: string) {
  if (current === null || previous === null) {
    return "-";
  }

  const delta = current - previous;
  const sign = delta >= 0 ? "+" : "";
  const value = suffix === "ops/s" ? Math.round(delta).toString() : delta.toFixed(1).replace(/\.0$/, "");

  return `${sign}${value} ${suffix}`;
}

function latencyDeltaLabel(current: number | null, previous: number | null) {
  const label = deltaLabel(current, previous, "ms");
  return label === "-" ? label : `${label} p95`;
}

export function Overview() {
  const snapshot = useOutletContext<WorkbenchSnapshot | null>();
  const latestRun = snapshot?.latestRun ?? null;
  const previousRun = snapshot?.previousRun ?? null;
  const serverSummary = snapshot ? summarizeWorkbenchStatus(snapshot) : "Loading local server state";

  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Overview / Control deck</p>
        <h1>Overview</h1>
        <p className="page__lede">
          Light situational awareness for the local HybridKV workbench: current server
          posture, the most recent benchmark signal, and shortcuts into the active lanes.
        </p>
      </div>

      <div className="metric-grid">
        <article className="metric-card">
          <p className="metric-card__label">Server state</p>
          <strong>{snapshot?.status.state ?? "loading"}</strong>
          <p>{serverSummary}</p>
        </article>
        <article className="metric-card">
          <p className="metric-card__label">Latest run</p>
          <strong>{latestRun ? summarizeLatestRun(snapshot) : "No benchmark run recorded yet"}</strong>
          <p>{latestRun ? `Target ${latestRun.targetAddr}` : "Launch your first benchmark from the workbench."}</p>
        </article>
        <article className="metric-card">
          <p className="metric-card__label">Live throughput</p>
          <strong>
            {snapshot?.info ? formatOpsPerSec(snapshot.info.instantaneousOpsPerSec) : formatOpsPerSec(latestRun?.throughputOpsPerSec)}
          </strong>
          <p>{snapshot?.info ? `INFO sample at ${snapshot.info.capturedAt}` : "Waiting for a running server or a recent run."}</p>
        </article>
      </div>

      <div className="page-panel-grid">
        <article className="panel panel--stacked">
          <p className="panel__label">Latest benchmark summary</p>
          <h2>{latestRun ? `Latest run ${latestRun.id} ${latestRun.status}` : "No benchmark run recorded yet"}</h2>
          <dl className="server-kv-list">
            <div>
              <dt>Throughput</dt>
              <dd>{formatOpsPerSec(latestRun?.throughputOpsPerSec)}</dd>
            </div>
            <div>
              <dt>P95 latency</dt>
              <dd>{formatLatencyMs(latestRun?.p95LatencyMs)}</dd>
            </div>
            <div>
              <dt>Runner</dt>
              <dd>{latestRun?.runner ?? "-"}</dd>
            </div>
            <div>
              <dt>Address</dt>
              <dd>{snapshot?.status.address ?? "127.0.0.1:6380"}</dd>
            </div>
          </dl>
        </article>

        <article className="panel panel--accent">
          <p className="panel__label">Recent trend</p>
          <h2>{latestRun && previousRun ? "Comparison highlights" : "Need two runs for a trend"}</h2>
          <dl className="server-kv-list">
            <div>
              <dt>Throughput delta</dt>
              <dd>{deltaLabel(latestRun?.throughputOpsPerSec ?? null, previousRun?.throughputOpsPerSec ?? null, "ops/s")}</dd>
            </div>
            <div>
              <dt>P95 delta</dt>
              <dd>{latencyDeltaLabel(latestRun?.p95LatencyMs ?? null, previousRun?.p95LatencyMs ?? null)}</dd>
            </div>
            <div>
              <dt>Recent run count</dt>
              <dd>{snapshot?.recentRuns.length ?? 0}</dd>
            </div>
            <div>
              <dt>Operator note</dt>
              <dd>{latestRun && previousRun ? "Use Runs for deeper comparison." : "Complete another run to unlock a trend."}</dd>
            </div>
          </dl>
        </article>
      </div>

      <div className="page-panel-grid">
        <article className="panel">
          <p className="panel__label">Quick actions</p>
          <h2>Jump straight into the active control surfaces</h2>
          <div className="server-actions" role="group" aria-label="Overview quick actions">
            <Link className="server-button server-button--primary" to="/benchmarks">
              Open benchmarks
            </Link>
            <Link className="server-button" to="/server">
              Open server controls
            </Link>
          </div>
        </article>

        <article className="panel">
          <p className="panel__label">Shared status flow</p>
          <h2>Shell and overview read the same workbench snapshot</h2>
          <p>
            The top bar mirrors the current server and benchmark snapshot so route changes do
            not hide the latest local state.
          </p>
        </article>
      </div>
    </section>
  );
}
