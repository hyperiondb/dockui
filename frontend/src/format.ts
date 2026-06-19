export function formatBytes(bytes: number): string {
  if (!bytes || bytes < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatPct(p: number): string {
  if (p >= 100) return `${p.toFixed(0)}%`;
  return `${p.toFixed(1)}%`;
}

export function cpuColor(pct: number, ncpu: number): string {
  const norm = Math.min(1, pct / (ncpu * 100));
  if (norm > 0.8) return "var(--danger)";
  if (norm > 0.5) return "var(--warn)";
  return "var(--accent)";
}

export function memColor(used: number, total: number): string {
  if (!total) return "var(--accent)";
  const r = used / total;
  if (r > 0.85) return "var(--danger)";
  if (r > 0.65) return "var(--warn)";
  return "var(--accent)";
}

export function stateColor(state: string): string {
  switch (state) {
    case "running":
      return "var(--ok)";
    case "paused":
      return "var(--warn)";
    case "restarting":
      return "var(--warn)";
    case "exited":
    case "dead":
      return "var(--muted)";
    default:
      return "var(--muted)";
  }
}
