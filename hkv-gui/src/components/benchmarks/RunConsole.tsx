import type { BenchmarkEventEnvelope } from "../../lib/types";

type RunConsoleProps = {
  activeRunId: string | null;
  events: BenchmarkEventEnvelope[];
};

export function RunConsole({ activeRunId, events }: RunConsoleProps) {
  return (
    <article className="panel panel--stacked benchmark-panel">
      <p className="panel__label">Terminal output</p>
      <h2>{activeRunId ?? "No active benchmark"}</h2>
      <p>
        Live stdout and stderr from the desktop runner stay scoped to the current
        benchmark session.
      </p>

      {events.length === 0 ? (
        <p>Waiting for benchmark events.</p>
      ) : (
        <div className="benchmark-console" role="log" aria-label="Benchmark console">
          {events.map((event, index) => (
            <div
              key={`${event.emittedAt}-${event.event}-${index}`}
              className={`benchmark-console__entry${event.error ? " benchmark-console__entry--error" : ""}`}
            >
              <strong>{event.event}</strong>
              <span>{event.message ?? event.error ?? "event received"}</span>
            </div>
          ))}
        </div>
      )}
    </article>
  );
}
