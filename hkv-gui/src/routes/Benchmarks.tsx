import { useEffect, useMemo, useRef, useState } from "react";

import { BenchmarkForm } from "../components/benchmarks/BenchmarkForm";
import { MetricsCards } from "../components/benchmarks/MetricsCards";
import { ResultCharts } from "../components/benchmarks/ResultCharts";
import { RunConsole } from "../components/benchmarks/RunConsole";
import {
  applyBenchmarkProfile,
  buildBenchmarkRequest,
  DEFAULT_BENCHMARK_FORM,
  type BenchmarkFormValues,
  validateBenchmarkForm
} from "../components/benchmarks/form-model";
import { getRunDetail, onBenchmarkEvent, startBenchmark, stopBenchmark } from "../lib/api";
import type { BenchmarkEventEnvelope, BenchmarkRun } from "../lib/types";

function mergeError(current: BenchmarkRun | null, message: string): BenchmarkRun | null {
  if (!current) {
    return null;
  }

  return { ...current, errorMessage: message };
}

export function Benchmarks() {
  const [formValues, setFormValues] = useState(DEFAULT_BENCHMARK_FORM);
  const [formErrors, setFormErrors] = useState<Partial<Record<keyof BenchmarkFormValues, string>>>({});
  const [busy, setBusy] = useState(false);
  const [activeRun, setActiveRun] = useState<BenchmarkRun | null>(null);
  const [consoleEvents, setConsoleEvents] = useState<BenchmarkEventEnvelope[]>([]);
  const activeRunIdRef = useRef<string | null>(null);
  const refreshInFlightRef = useRef(false);
  const refreshPendingRef = useRef(false);

  useEffect(() => {
    activeRunIdRef.current = activeRun?.id ?? null;
  }, [activeRun]);

  useEffect(() => {
    let mounted = true;
    let teardown: (() => void) | undefined;

    const refreshRunDetail = async (runId: string) => {
      if (refreshInFlightRef.current) {
        refreshPendingRef.current = true;
        return;
      }

      refreshInFlightRef.current = true;

      try {
        const detail = await getRunDetail(runId);
        if (mounted) {
          setActiveRun(detail);
        }
      } catch (error) {
        if (!mounted) {
          return;
        }

        const message = error instanceof Error ? error.message : "Unable to refresh benchmark detail";
        setActiveRun((current) => mergeError(current, message));
      } finally {
        refreshInFlightRef.current = false;
        if (refreshPendingRef.current && mounted && activeRunIdRef.current === runId) {
          refreshPendingRef.current = false;
          void refreshRunDetail(runId);
        }
      }
    };

    const register = async () => {
      try {
        teardown = await onBenchmarkEvent(async (payload) => {
          if (!mounted) {
            return;
          }

          if (!activeRunIdRef.current || payload.runId !== activeRunIdRef.current) {
            return;
          }

          setConsoleEvents((current) => [...current.slice(-23), payload]);
          await refreshRunDetail(payload.runId);
        });
      } catch {
        teardown = undefined;
      }
    };

    void register();

    return () => {
      mounted = false;
      teardown?.();
    };
  }, []);

  const handleFormChange = (field: keyof BenchmarkFormValues, value: string) => {
    setFormValues((current) => {
      const next = { ...current, [field]: value };
      if (field === "profile") {
        return applyBenchmarkProfile(value, next);
      }

      return next;
    });
  };

  const handleStart = async () => {
    const nextErrors = validateBenchmarkForm(formValues);
    setFormErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      return;
    }

    setBusy(true);
    try {
      const startedRun = await startBenchmark(buildBenchmarkRequest(formValues));
      setActiveRun(startedRun);
      setConsoleEvents([]);
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to start benchmark";
      setActiveRun((current) =>
        current
          ? { ...current, errorMessage: message }
          : {
              id: "pending-run",
              request: buildBenchmarkRequest(formValues),
              status: "failed",
              createdAt: new Date(0).toISOString(),
              startedAt: null,
              finishedAt: null,
              result: null,
              errorMessage: message
            }
      );
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    if (!activeRun) {
      return;
    }

    setBusy(true);
    try {
      const stoppedRun = await stopBenchmark(activeRun.id);
      setActiveRun(stoppedRun);
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to stop benchmark";
      setActiveRun((current) => mergeError(current, message));
    } finally {
      setBusy(false);
    }
  };

  const summaryItems = useMemo(
    () => [
      ["Run", activeRun?.id ?? "No run yet"],
      ["Status", activeRun?.status ?? "idle"],
      ["Target", activeRun?.request.targetAddr ?? `${formValues.host}:${formValues.port}`],
      ["Runner", activeRun?.request.runner ?? "redis-benchmark"]
    ],
    [activeRun, formValues.host, formValues.port]
  );

  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Benchmarks / Orchestration lane</p>
        <h1>Benchmarks</h1>
        <p className="page__lede">
          Launch and observe real benchmark runs through the existing Tauri
          command surface, with just enough live telemetry for a useful v1.
        </p>
      </div>

      <div className="page-panel-grid">
        <BenchmarkForm
          busy={busy}
          errors={formErrors}
          values={formValues}
          onChange={handleFormChange}
          onSubmit={handleStart}
        />

        <article className="panel panel--stacked benchmark-panel">
          <p className="panel__label">Active run</p>
          <h2>{activeRun?.id ?? "Ready to launch"}</h2>
          <p>
            Current run identity, execution state, and shared target context for the
            workbench panels below.
          </p>

          <dl className="server-kv-list">
            {summaryItems.map(([label, value]) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>

          <div className="server-actions" role="group" aria-label="Active benchmark controls">
            <button
              type="button"
              className="server-button"
              onClick={handleStop}
              disabled={busy || !activeRun}
            >
              Stop benchmark
            </button>
          </div>

          {activeRun?.errorMessage ? <p className="server-error">{activeRun.errorMessage}</p> : null}
        </article>
      </div>

      <MetricsCards run={activeRun} />

      <div className="page-panel-grid">
        <RunConsole activeRunId={activeRun?.id ?? null} events={consoleEvents} />
        <ResultCharts run={activeRun} />
      </div>
    </section>
  );
}
