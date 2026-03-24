export function formatInteger(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "-";
  }

  return Math.round(value).toLocaleString("en-US", { useGrouping: false });
}

export function formatDecimal(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined) {
    return "-";
  }

  return value.toLocaleString("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: digits,
    useGrouping: false
  });
}

export function formatOpsPerSec(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "-";
  }

  return `${formatDecimal(value, 0)} ops/s`;
}

export function formatLatencyMs(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "-";
  }

  return `${formatDecimal(value, 1)} ms`;
}

export function formatDurationSeconds(durationMs: number | null | undefined): string {
  if (durationMs === null || durationMs === undefined) {
    return "-";
  }

  return `${formatDecimal(durationMs / 1000, 0)} s`;
}

export function formatBytes(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "-";
  }

  if (value < 1024) {
    return `${formatInteger(value)} B`;
  }

  const units = ["KiB", "MiB", "GiB", "TiB"];
  let size = value / 1024;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }

  return `${formatDecimal(size, 1)} ${units[unitIndex]}`;
}

export function formatTerminalLineCount(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "-";
  }

  return `${formatInteger(value)} terminal ${value === 1 ? "line" : "lines"} captured`;
}
