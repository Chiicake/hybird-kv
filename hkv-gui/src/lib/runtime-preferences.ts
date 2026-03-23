export type RuntimePreferences = {
  benchmarkBinaryPath: string;
  benchmarkTargetHost: string;
  benchmarkTargetPort: string;
};

export const DEFAULT_RUNTIME_PREFERENCES: RuntimePreferences = {
  benchmarkBinaryPath: "",
  benchmarkTargetHost: "127.0.0.1",
  benchmarkTargetPort: "6379"
};

export const RUNTIME_PREFERENCES_STORAGE_KEY = "hkv-gui.runtime-preferences.v1";

function isBrowserEnvironment() {
  return typeof window !== "undefined" && typeof window.localStorage !== "undefined";
}

function normalizeString(value: unknown, fallback: string) {
  if (typeof value !== "string") {
    return fallback;
  }

  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : fallback;
}

export function sanitizeRuntimePreferences(input: Partial<RuntimePreferences>): RuntimePreferences {
  return {
    benchmarkBinaryPath:
      typeof input.benchmarkBinaryPath === "string" ? input.benchmarkBinaryPath.trim() : "",
    benchmarkTargetHost: normalizeString(
      input.benchmarkTargetHost,
      DEFAULT_RUNTIME_PREFERENCES.benchmarkTargetHost
    ),
    benchmarkTargetPort: normalizeString(
      input.benchmarkTargetPort,
      DEFAULT_RUNTIME_PREFERENCES.benchmarkTargetPort
    )
  };
}

export function loadRuntimePreferences(): RuntimePreferences {
  if (!isBrowserEnvironment()) {
    return DEFAULT_RUNTIME_PREFERENCES;
  }

  try {
    const raw = window.localStorage.getItem(RUNTIME_PREFERENCES_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_RUNTIME_PREFERENCES;
    }

    const parsed = JSON.parse(raw) as Partial<RuntimePreferences>;
    return sanitizeRuntimePreferences(parsed);
  } catch {
    return DEFAULT_RUNTIME_PREFERENCES;
  }
}

export function saveRuntimePreferences(input: Partial<RuntimePreferences>) {
  const next = sanitizeRuntimePreferences(input);
  if (!isBrowserEnvironment()) {
    return next;
  }

  window.localStorage.setItem(RUNTIME_PREFERENCES_STORAGE_KEY, JSON.stringify(next));
  return next;
}

export function resolveBenchmarkBinaryPath(preferences: RuntimePreferences): string {
  return preferences.benchmarkBinaryPath || "redis-benchmark";
}
