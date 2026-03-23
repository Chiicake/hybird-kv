import { useEffect, useState } from "react";

import { InfoSummaryPanel } from "../components/server/InfoSummaryPanel";
import { ServerControlPanel } from "../components/server/ServerControlPanel";
import { ServerStatusPanel } from "../components/server/ServerStatusPanel";
import {
  currentInfoSnapshot,
  serverStatus,
  startServer,
  stopServer
} from "../lib/api";
import type { InfoSnapshot, ServerStatus as ServerStatusType } from "../lib/types";

const DEFAULT_STATUS: ServerStatusType = {
  state: "stopped",
  address: "127.0.0.1:6380",
  pid: null,
  startedAt: null,
  lastError: null
};

const AUTO_REFRESH_ENABLED = import.meta.env.MODE !== "test";

export function Server() {
  const [status, setStatus] = useState<ServerStatusType>(DEFAULT_STATUS);
  const [snapshot, setSnapshot] = useState<InfoSnapshot | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!AUTO_REFRESH_ENABLED) {
      return undefined;
    }

    let active = true;

    const refresh = async () => {
      try {
        const nextStatus = await serverStatus();
        if (!active) {
          return;
        }

        setStatus(nextStatus);

        if (nextStatus.state === "running") {
          const nextInfo = await currentInfoSnapshot();
          if (active) {
            setSnapshot(nextInfo);
          }
        } else if (active) {
          setSnapshot(null);
        }
      } catch (error) {
        if (!active) {
          return;
        }

        const message = error instanceof Error ? error.message : "Unable to refresh server state";
        setStatus((current) => ({ ...current, lastError: message }));
      }
    };

    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, 1500);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const handleStart = async () => {
    setBusy(true);
    try {
      const nextStatus = await startServer({ address: "127.0.0.1", port: 6380 });
      setStatus(nextStatus);
      const nextInfo = await currentInfoSnapshot();
      setSnapshot(nextInfo);
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to start local server";
      setStatus((current) => ({ ...current, lastError: message }));
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    setBusy(true);
    try {
      const nextStatus = await stopServer();
      setStatus(nextStatus);
      setSnapshot(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to stop local server";
      setStatus((current) => ({ ...current, lastError: message }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="page">
      <div className="page__hero">
        <p className="page__eyebrow">Server / Local runtime lane</p>
        <h1>Server</h1>
        <p className="page__lede">
          Local process control for `hkv-server`, plus a lightweight INFO view
          for the active endpoint.
        </p>
      </div>

      <div className="page-panel-grid">
        <ServerControlPanel
          busy={busy}
          status={status}
          onStart={handleStart}
          onStop={handleStop}
        />
        <ServerStatusPanel status={status} />
      </div>

      <InfoSummaryPanel snapshot={snapshot} />
    </section>
  );
}
