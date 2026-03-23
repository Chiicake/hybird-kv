import type { ServerStatus } from "../../lib/types";

type ServerControlPanelProps = {
  busy: boolean;
  status: ServerStatus;
  onStart: () => void;
  onStop: () => void;
};

export function ServerControlPanel({
  busy,
  status,
  onStart,
  onStop
}: ServerControlPanelProps) {
  const isRunning = status.state === "running";

  return (
    <article className="panel panel--accent server-panel">
      <p className="panel__label">Local process control</p>
      <h2>{isRunning ? "HybridKV node is live" : "HybridKV node is idle"}</h2>
      <p>
        Launches the local `hkv-server` binary with the configured address and
        keeps the current process status visible from the desktop shell.
      </p>

      <div className="server-actions" role="group" aria-label="Server actions">
        <button
          type="button"
          className="server-button server-button--primary"
          onClick={onStart}
          disabled={busy || isRunning}
        >
          {busy && !isRunning ? "Starting..." : "Start local server"}
        </button>
        <button
          type="button"
          className="server-button"
          onClick={onStop}
          disabled={busy || !isRunning}
        >
          {busy && isRunning ? "Stopping..." : "Stop server"}
        </button>
      </div>
    </article>
  );
}
