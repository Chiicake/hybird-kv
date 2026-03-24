import { useEffect, useMemo, useRef, useState } from "react";

import { BenchmarkForm } from "../components/benchmarks/BenchmarkForm";
import { MetricsCards } from "../components/benchmarks/MetricsCards";
import { ResultCharts } from "../components/benchmarks/ResultCharts";
import { RunConsole } from "../components/benchmarks/RunConsole";
import {
  applyBenchmarkProfile,
  buildBenchmarkRequest,
  createBenchmarkFormDefaults,
  type BenchmarkFormValues,
  validateBenchmarkForm
} from "../components/benchmarks/form-model";
import { getRunDetail, onBenchmarkEvent, startBenchmark, stopBenchmark } from "../lib/api";
import { formatTerminalLineCount } from "../lib/format";
import type { BenchmarkEventEnvelope, BenchmarkRun } from "../lib/types";

function formatRequestBudget(value: number) {
  if (value >= 1_000_000) {
    return `${Math.round(value / 1_000_000)}M requests`;
  }

  if (value >= 1_000) {
    return `${Math.round(value / 1_000)}k requests`;
  }

  return `${value} requests`;
}

function buildPhase2aHelper(run: BenchmarkRun | null, terminalLineCount: number) {
  if (!run?.result) {
    return null;
  }

  const { request } = run;

  return {
    comparisonLabel: `${request.runner} ${request.clients}c x ${formatRequestBudget(request.requests)}`,
    workloadSummary: `${formatRequestBudget(request.requests)}, ${request.clients} clients, ${request.dataSize} B payload, pipeline ${request.pipeline}`,
    terminalLineCount
  };
}

function mergeError(current: BenchmarkRun | null, message: string): BenchmarkRun | null {
  if (!current) {
    return null;
  }

  return { ...current, errorMessage: message };
}

function formatBenchmarkStartError(message: string, targetAddr: string) {
  const normalized = message.trim();

  if (/connection refused/i.test(normalized)) {
    return `${normalized}. Make sure a Redis-compatible server is running at ${targetAddr}. If you want to use the built-in GUI server, start it from the Server page first.`;
  }

  if (/binary_missing|failed to start redis-benchmark/i.test(normalized)) {
    return `${normalized}. Install redis-benchmark or set a valid benchmark executable override in Settings.`;
  }

  return `${normalized}. Check that the target server is reachable at ${targetAddr}.`;
}

function formatBenchmarkStopError(message: string) {
  const normalized = message.trim();

  if (/run_not_active/i.test(normalized)) {
    return "The benchmark run is no longer active. It may have already finished or failed.";
  }

  return normalized;
}

export function Benchmarks() {
  const [formValues, setFormValues] = useState(createBenchmarkFormDefaults);
  const [formErrors, setFormErrors] = useState<Partial<Record<keyof BenchmarkFormValues, string>>>({});
  const [busy, setBusy] = useState(false);
  const [activeRun, setActiveRun] = useState<BenchmarkRun | null>(null);
  const [consoleEvents, setConsoleEvents] = useState<BenchmarkEventEnvelope[]>([]);
  const [terminalLineCount, setTerminalLineCount] = useState(0);
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

          setTerminalLineCount((current) => current + 1);
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
      setTerminalLineCount(0);
    } catch (error) {
      const rawMessage = error instanceof Error ? error.message : "Unable to start benchmark";
      const message = formatBenchmarkStartError(
        rawMessage,
        `${formValues.host}:${formValues.port}`
      );
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
      const rawMessage = error instanceof Error ? error.message : "Unable to stop benchmark";
      const message = formatBenchmarkStopError(rawMessage);
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

  const phase2aHelper = buildPhase2aHelper(activeRun, terminalLineCount);

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

      <div className="benchmark-terminal-layout">
        <RunConsole activeRunId={activeRun?.id ?? null} events={consoleEvents} />

        <div className="benchmark-terminal-layout__sidebar">
          {phase2aHelper ? (
            <article className="panel panel--stacked benchmark-panel">
              <p className="panel__label">Experiment helper</p>
              <h2>Phase 2A capture</h2>
              <p>
                Lightweight suggestions make it easier to tag terminal-first runs
                for later Phase 2A comparison without treating the GUI as the
                source of truth.
              </p>

              <dl className="server-kv-list">
                <div>
                  <dt>Suggested label</dt>
                  <dd>{phase2aHelper.comparisonLabel}</dd>
                </div>
                <div>
                  <dt>Workload</dt>
                  <dd>{phase2aHelper.workloadSummary}</dd>
                </div>
                <div>
                  <dt>Capture</dt>
                  <dd>{formatTerminalLineCount(phase2aHelper.terminalLineCount)}</dd>
                </div>
              </dl>
            </article>
          ) : null}

          <MetricsCards run={activeRun} />
          <ResultCharts run={activeRun} />
        </div>
      </div>
    </section>
  );
}
