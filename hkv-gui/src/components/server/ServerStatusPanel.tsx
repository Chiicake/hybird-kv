import type { ServerStatus } from "../../lib/types";

type ServerStatusPanelProps = {
  status: ServerStatus;
};

export function ServerStatusPanel({ status }: ServerStatusPanelProps) {
  const isRunning = status.state === "running";
  const [host, port] = status.address.split(":");

  return (
    <article className="panel panel--stacked server-panel">
      <p className="panel__label">Runtime status</p>
      <h2>{isRunning ? "Process attached" : "No process attached"}</h2>

      <dl className="server-kv-list">
        <div>
          <dt>State</dt>
          <dd>{status.state}</dd>
        </div>
        <div>
          <dt>Host</dt>
          <dd>{host ?? status.address}</dd>
        </div>
        <div>
          <dt>Port</dt>
          <dd>{port ?? "-"}</dd>
        </div>
        <div>
          <dt>PID</dt>
          <dd>{status.pid ?? "-"}</dd>
        </div>
        <div>
          <dt>Started</dt>
          <dd>{status.startedAt ?? "-"}</dd>
        </div>
      </dl>

      {status.lastError ? <p className="server-error">{status.lastError}</p> : null}
    </article>
  );
}
