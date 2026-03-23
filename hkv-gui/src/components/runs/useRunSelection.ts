import { useEffect, useMemo, useState } from "react";

import { getRunDetail } from "../../lib/api";
import type { BenchmarkRun } from "../../lib/types";

export function useRunSelection(selectedRunId: string | null, compareSelection: string[]) {
  const [selectedRun, setSelectedRun] = useState<BenchmarkRun | null>(null);
  const [compareDetails, setCompareDetails] = useState<Record<string, BenchmarkRun>>({});

  useEffect(() => {
    if (!selectedRunId) {
      setSelectedRun(null);
      return;
    }

    if (compareDetails[selectedRunId]) {
      setSelectedRun(compareDetails[selectedRunId]);
      return;
    }

    let active = true;

    const loadDetail = async () => {
      const run = await getRunDetail(selectedRunId);
      if (active) {
        setSelectedRun(run);
        setCompareDetails((current) => ({ ...current, [selectedRunId]: run }));
      }
    };

    void loadDetail();

    return () => {
      active = false;
    };
  }, [compareDetails, selectedRunId]);

  useEffect(() => {
    let active = true;
    const missingIds = compareSelection.filter((runId) => !compareDetails[runId]);
    if (missingIds.length === 0) {
      return undefined;
    }

    const loadMissingDetails = async () => {
      const loadedEntries = await Promise.all(
        missingIds.map(async (runId) => [runId, await getRunDetail(runId)] as const)
      );
      if (!active) {
        return;
      }

      setCompareDetails((current) => {
        const next = { ...current };
        loadedEntries.forEach(([runId, run]) => {
          next[runId] = run;
        });
        return next;
      });
    };

    void loadMissingDetails();

    return () => {
      active = false;
    };
  }, [compareDetails, compareSelection]);

  const comparisonRuns = useMemo(
    () => compareSelection.map((runId) => compareDetails[runId]).filter(Boolean),
    [compareDetails, compareSelection]
  );

  return {
    selectedRun,
    comparisonRuns
  };
}
