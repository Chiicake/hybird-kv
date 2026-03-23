import { useEffect, useState } from "react";

import {
  loadWorkbenchSnapshot,
  onBenchmarkEvent,
  onServerEvent,
  type WorkbenchSnapshot
} from "../../lib/api";

export function useWorkbenchSnapshot() {
  const [snapshot, setSnapshot] = useState<WorkbenchSnapshot | null>(null);

  useEffect(() => {
    let active = true;
    let benchmarkUnlisten: (() => void) | null = null;
    let serverUnlisten: (() => void) | null = null;
    let disposed = false;

    const loadSnapshot = async () => {
      try {
        const nextSnapshot = await loadWorkbenchSnapshot();
        if (active) {
          setSnapshot(nextSnapshot);
        }
      } catch {
        if (active) {
          setSnapshot(null);
        }
      }
    };

    const register = async () => {
      const benchmarkStop = await onBenchmarkEvent(() => {
        void loadSnapshot();
      });
      if (disposed) {
        benchmarkStop();
      } else {
        benchmarkUnlisten = benchmarkStop;
      }

      const serverStop = await onServerEvent(() => {
        void loadSnapshot();
      });
      if (disposed) {
        serverStop();
      } else {
        serverUnlisten = serverStop;
      }
    };

    void loadSnapshot();
    void register();

    return () => {
      active = false;
      disposed = true;
      benchmarkUnlisten?.();
      serverUnlisten?.();
    };
  }, []);

  return snapshot;
}

export function summarizeWorkbenchStatus(snapshot: WorkbenchSnapshot | null) {
  if (!snapshot) {
    return "Loading local server and benchmark state";
  }

  if (snapshot.status.state === "running") {
    return `Server running at ${snapshot.status.address}`;
  }

  if (snapshot.status.lastError) {
    return `Server issue: ${snapshot.status.lastError}`;
  }

  return "Server is stopped";
}

export function summarizeLatestRun(snapshot: WorkbenchSnapshot | null) {
  if (!snapshot?.latestRun) {
    return "No benchmark run recorded";
  }

  return `Latest run ${snapshot.latestRun.id} ${snapshot.latestRun.status}`;
}
