import type { NormalizedRunSummary } from "../../lib/types";

type RunListProps = {
  compareSelection: string[];
  runs: NormalizedRunSummary[];
  selectedRunId: string | null;
  onCompareToggle: (runId: string) => void;
  onSelectRun: (runId: string) => void;
};

function metricValue(value: number | null, suffix: string) {
  return value === null ? "-" : `${value} ${suffix}`;
}

export function RunList({
  compareSelection,
  runs,
  selectedRunId,
  onCompareToggle,
  onSelectRun
}: RunListProps) {
  return (
    <article className="panel panel--stacked runs-panel">
      <p className="panel__label">Run history</p>
      <h2>Persisted sessions</h2>
      <p>Recent completed and failed benchmark runs stored by the desktop backend.</p>

      {runs.length === 0 ? (
        <p>No persisted benchmark runs yet.</p>
      ) : (
        <div className="runs-timeline" role="list" aria-label="Persisted benchmark timeline">
          {runs.map((run) => {
            const isSelected = selectedRunId === run.id;
            const isCompared = compareSelection.includes(run.id);
            const compareDisabled = !isCompared && compareSelection.length >= 2;

            return (
              <div
                key={run.id}
                className={`runs-list__item${isSelected ? " runs-list__item--selected" : ""}`}
                role="listitem"
              >
                <div className="runs-list__timeline-marker" aria-hidden="true" />
                <div className="runs-list__header">
                  <div>
                    <strong>{run.id}</strong>
                    <p>{run.runner}</p>
                  </div>
                  <span className={`runs-status runs-status--${run.status}`}>{run.status}</span>
                </div>

                <dl className="runs-list__metrics">
                  <div>
                    <dt>Created</dt>
                    <dd>{run.createdAt}</dd>
                  </div>
                  <div>
                    <dt>Target</dt>
                    <dd>{run.targetAddr}</dd>
                  </div>
                  <div>
                    <dt>Throughput</dt>
                    <dd>{metricValue(run.throughputOpsPerSec, "ops/s")}</dd>
                  </div>
                  <div>
                    <dt>P95</dt>
                    <dd>{metricValue(run.p95LatencyMs, "ms")}</dd>
                  </div>
                </dl>

                <div className="runs-list__actions">
                  <button
                    type="button"
                    className="server-button server-button--primary"
                    onClick={() => onSelectRun(run.id)}
                    aria-label={`Select run ${run.id}`}
                  >
                    {isSelected ? "Viewing" : "Select run"}
                  </button>
                  <label className="runs-compare-toggle">
                    <input
                      type="checkbox"
                      checked={isCompared}
                      disabled={compareDisabled}
                      onChange={() => onCompareToggle(run.id)}
                      aria-label={`Compare ${run.id}`}
                    />
                    Compare
                  </label>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </article>
  );
}
