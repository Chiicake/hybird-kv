import type { BenchmarkEventEnvelope } from "../../lib/types";

type RunConsoleProps = {
  activeRunId: string | null;
  events: BenchmarkEventEnvelope[];
};

export function RunConsole({ activeRunId, events }: RunConsoleProps) {
  return (
    <article className="panel panel--stacked benchmark-panel">
      <p className="panel__label">Run console</p>
      <h2>{activeRunId ?? "No active benchmark"}</h2>
      <p>
        Lifecycle messages stream from the desktop backend and stay scoped to the
        currently active run.
      </p>

      {events.length === 0 ? (
        <p>Waiting for benchmark events.</p>
      ) : (
        <div className="benchmark-console" role="log" aria-label="Benchmark console">
          {events.map((event, index) => (
            <div key={`${event.emittedAt}-${event.event}-${index}`} className="benchmark-console__entry">
              <strong>{event.event}</strong>
              <span>{event.message ?? event.error ?? "event received"}</span>
            </div>
          ))}
        </div>
      )}
    </article>
  );
}
