import type { ContainerInfo, ContainerStat } from "../types";
import { cpuColor, formatBytes, formatPct, stateColor } from "../format";
import { Sparkline } from "./Sparkline";

interface Props {
  containers: ContainerInfo[];
  stats: Map<string, ContainerStat>;
  cpuHistory: Map<string, number[]>;
  ncpu: number;
  selectedId: string | null;
  filter: string;
  onSelect: (id: string) => void;
  onFilter: (v: string) => void;
}

export function ContainerList({
  containers,
  stats,
  cpuHistory,
  ncpu,
  selectedId,
  filter,
  onSelect,
  onFilter,
}: Props) {
  const f = filter.trim().toLowerCase();
  const shown = f
    ? containers.filter(
        (c) => c.name.toLowerCase().includes(f) || c.image.toLowerCase().includes(f),
      )
    : containers;

  return (
    <aside className="sidebar">
      <div className="filter">
        <input
          type="text"
          placeholder="Filter containers…"
          value={filter}
          onChange={(e) => onFilter(e.target.value)}
        />
      </div>
      <div className="clist">
        {shown.length === 0 && <div className="empty">No containers</div>}
        {shown.map((c) => {
          const s = stats.get(c.id);
          const hist = cpuHistory.get(c.id) ?? [];
          const cpu = s?.cpu_pct ?? 0;
          const mem = s?.mem_bytes ?? 0;
          const isRunning = c.state === "running";
          return (
            <button
              key={c.id}
              className={`crow ${selectedId === c.id ? "active" : ""} ${isRunning ? "" : "stopped"}`}
              onClick={() => onSelect(c.id)}
            >
              <span className="dot" style={{ background: stateColor(c.state) }} />
              <span className="cinfo">
                <span className="cname" title={c.name}>
                  {c.name}
                </span>
                <span className="cimage" title={c.image}>
                  {c.image}
                </span>
              </span>
              {isRunning ? (
                <span className="cmetrics">
                  <span className="cnums">
                    <span
                      className="cpu"
                      style={{ color: cpuColor(cpu, ncpu) }}
                      title={`${formatPct(cpu / ncpu)} of host · ${formatPct(cpu)} of one core · ${ncpu} core${ncpu === 1 ? "" : "s"}`}
                    >
                      {formatPct(cpu / ncpu)}
                    </span>
                    <span className="mem">{formatBytes(mem)}</span>
                  </span>
                  <Sparkline values={hist} color={cpuColor(cpu, ncpu)} max={ncpu * 100} />
                </span>
              ) : (
                <span className="cmetrics">
                  <span className="cstate">{c.state}</span>
                </span>
              )}
            </button>
          );
        })}
      </div>
    </aside>
  );
}
