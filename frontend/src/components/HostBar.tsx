import type { HostStat } from "../types";
import { cpuColor, formatBytes, formatPct, memColor } from "../format";

interface Props {
  host: HostStat | null;
  running: number;
  total: number;
  connected: boolean;
  onHome: () => void;
}

export function HostBar({ host, running, total, connected, onHome }: Props) {
  const ncpu = host?.ncpu ?? 1;
  const cpu = host?.cpu_pct ?? 0;
  const memUsed = host?.mem_used ?? 0;
  const memTotal = host?.mem_total ?? 0;
  const memRatio = memTotal ? memUsed / memTotal : 0;

  return (
    <header className="hostbar">
      <button className="brand" onClick={onHome} title="Host overview (home)">
        <span className="logo">▦</span> dockui
      </button>

      <div className="gauge">
        <div className="gauge-head">
          <span>HOST CPU</span>
          <span className="gauge-val">{formatPct(cpu)}</span>
        </div>
        <div className="meter">
          <div
            className="meter-fill"
            style={{ width: `${Math.min(100, cpu)}%`, background: cpuColor(cpu, 1) }}
          />
        </div>
        <div className="gauge-sub">{ncpu} cores</div>
      </div>

      <div className="gauge">
        <div className="gauge-head">
          <span>HOST MEM</span>
          <span className="gauge-val">
            {formatBytes(memUsed)} / {formatBytes(memTotal)}
          </span>
        </div>
        <div className="meter">
          <div
            className="meter-fill"
            style={{
              width: `${Math.min(100, memRatio * 100)}%`,
              background: memColor(memUsed, memTotal),
            }}
          />
        </div>
        <div className="gauge-sub">{formatPct(memRatio * 100)} used</div>
      </div>

      <div className="host-meta">
        <div className="pill">
          <strong>{running}</strong> running
        </div>
        <div className="pill">
          <strong>{total}</strong> total
        </div>
        <div className={`status-dot ${connected ? "live" : "down"}`} title={connected ? "live" : "disconnected"} />
      </div>
    </header>
  );
}
