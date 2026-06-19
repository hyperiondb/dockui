import { useEffect, useState } from "react";
import uPlot from "uplot";
import type { Range } from "../types";
import { fetchContainerHistory } from "../api";
import { formatBytes } from "../format";
import { Chart } from "./Chart";

const RANGES: Range[] = ["15m", "1h", "6h", "24h", "7d"];
const CPU_COLOR = "#58a6ff";
const MEM_COLOR = "#3fb950";
const EMPTY: uPlot.AlignedData = [[], []];

interface Props {
  containerId: string;
  ncpu: number;
}

export function MetricsCharts({ containerId, ncpu }: Props) {
  const [range, setRange] = useState<Range>("1h");
  const [cpu, setCpu] = useState<uPlot.AlignedData>(EMPTY);
  const [mem, setMem] = useState<uPlot.AlignedData>(EMPTY);

  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const pts = await fetchContainerHistory(containerId, range);
        if (!alive) return;
        const xs = pts.map((p) => p.ts / 1000);
        const cores = ncpu || 1;
        setCpu([xs, pts.map((p) => p.cpu_pct / cores)]);
        setMem([xs, pts.map((p) => p.mem_bytes)]);
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
  }, [containerId, range, ncpu]);

  return (
    <div className="metrics">
      <div className="metrics-head">
        <span className="metrics-title">Metrics</span>
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
      <div className="charts">
        <div className="chart-card">
          <div className="chart-label">CPU % of host</div>
          <Chart data={cpu} color={CPU_COLOR} fill="rgba(88,166,255,0.13)" yFmt={(v) => `${v.toFixed(0)}%`} />
        </div>
        <div className="chart-card">
          <div className="chart-label">Memory</div>
          <Chart data={mem} color={MEM_COLOR} fill="rgba(63,185,80,0.13)" yFmt={formatBytes} />
        </div>
      </div>
    </div>
  );
}
