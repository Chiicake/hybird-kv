import { useEffect, useState } from "react";

import { RunComparison } from "../components/runs/RunComparison";
import { RunDetail } from "../components/runs/RunDetail";
import { RunList } from "../components/runs/RunList";
import { useRunSelection } from "../components/runs/useRunSelection";
import { listRuns } from "../lib/api";
import type { NormalizedRunSummary } from "../lib/types";

export function Runs() {
  const [runs, setRuns] = useState<NormalizedRunSummary[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [compareSelection, setCompareSelection] = useState<string[]>([]);
  const { selectedRun, comparisonRuns } = useRunSelection(selectedRunId, compareSelection);

  useEffect(() => {
    let active = true;

    const loadRuns = async () => {
      const nextRuns = await listRuns();
      if (!active) {
        return;
      }

      setRuns(nextRuns);
      if (nextRuns.length > 0) {
        setSelectedRunId(nextRuns[0].id);
      }
    };

    void loadRuns();

    return () => {
      active = false;
    };
  }, []);

  const handleCompareToggle = (runId: string) => {
    setCompareSelection((current) => {
      if (current.includes(runId)) {
        return current.filter((value) => value !== runId);
      }

      if (current.length >= 2) {
        return current;
      }

      return [...current, runId];
    });
  };

  const handleSelectRun = (runId: string) => {
    setSelectedRunId(runId);
    setCompareSelection((current) => {
      if (current.includes(runId) || current.length !== 1) {
        return current;
      }

      return [...current, runId];
    });
  };

  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Runs / Archive lane</p>
        <h1>Runs</h1>
        <p className="page__lede">
          Persisted benchmark history, single-run inspection, and a modest
          two-run comparison flow for v1.
        </p>
      </div>

      <div className="page-panel-grid">
        <RunList
          compareSelection={compareSelection}
          runs={runs}
          selectedRunId={selectedRunId}
          onCompareToggle={handleCompareToggle}
          onSelectRun={handleSelectRun}
        />
        <RunDetail run={selectedRun} />
      </div>

      <RunComparison runs={comparisonRuns} />
    </section>
  );
}
