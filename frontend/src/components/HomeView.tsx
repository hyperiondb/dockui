import { useEffect, useState } from "react";
import uPlot from "uplot";
import type { HostStat, Range } from "../types";
import { fetchHostHistory } from "../api";
import { formatBytes, formatPct } from "../format";
import { Chart } from "./Chart";

const RANGES: Range[] = ["15m", "1h", "6h", "24h", "7d"];
const CPU_COLOR = "#58a6ff";
const MEM_COLOR = "#3fb950";
const EMPTY: uPlot.AlignedData = [[], []];

interface Props {
  host: HostStat | null;
  running: number;
  total: number;
}

export function HomeView({ host, running, total }: Props) {
  const [range, setRange] = useState<Range>("1h");
  const [cpu, setCpu] = useState<uPlot.AlignedData>(EMPTY);
  const [mem, setMem] = useState<uPlot.AlignedData>(EMPTY);

  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const pts = await fetchHostHistory(range);
        if (!alive) return;
        const xs = pts.map((p) => p.ts / 1000);
        setCpu([xs, pts.map((p) => p.cpu_pct)]);
        setMem([xs, pts.map((p) => p.mem_used)]);
      } catch {
        if (alive) {
          setCpu(EMPTY);
          setMem(EMPTY);
        }
      }
    };
    load();
    const t = setInterval(load, 15000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [range]);

  return (
    <div className="home">
      <div className="home-head">
        <h2 className="home-title">Host overview</h2>
        <div className="ranges">
          {RANGES.map((r) => (
            <button
              key={r}
              className={`rbtn ${range === r ? "on" : ""}`}
              onClick={() => setRange(r)}
            >
              {r}
            </button>
          ))}
        </div>
      </div>

      <div className="home-cards">
        <div className="stat-card">
          <span className="stat-k">CPU</span>
          <span className="stat-v">{formatPct(host?.cpu_pct ?? 0)}</span>
        </div>
        <div className="stat-card">
          <span className="stat-k">Memory</span>
          <span className="stat-v">
            {formatBytes(host?.mem_used ?? 0)} <span className="stat-sub">/ {formatBytes(host?.mem_total ?? 0)}</span>
          </span>
        </div>
        <div className="stat-card">
          <span className="stat-k">Cores</span>
          <span className="stat-v">{host?.ncpu ?? "—"}</span>
        </div>
        <div className="stat-card">
          <span className="stat-k">Containers</span>
          <span className="stat-v">
            {running} <span className="stat-sub">/ {total}</span>
          </span>
        </div>
      </div>

      <div className="charts">
        <div className="chart-card">
          <div className="chart-label">Host CPU %</div>
          <Chart data={cpu} color={CPU_COLOR} fill="rgba(88,166,255,0.13)" yFmt={(v) => `${v.toFixed(0)}%`} />
        </div>
        <div className="chart-card">
          <div className="chart-label">Host memory</div>
          <Chart data={mem} color={MEM_COLOR} fill="rgba(63,185,80,0.13)" yFmt={formatBytes} />
        </div>
      </div>

      <p className="home-hint">Select a container on the left to view its logs and metrics.</p>
    </div>
  );
}
