import { useEffect, useRef, useState } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { Range } from "../types";
import { fetchContainerHistory } from "../api";
import { formatBytes } from "../format";

const RANGES: Range[] = ["15m", "1h", "6h", "24h", "7d"];
const CPU_COLOR = "#58a6ff";
const MEM_COLOR = "#3fb950";

interface ChartProps {
  data: uPlot.AlignedData;
  color: string;
  fill: string;
  yFmt: (v: number) => string;
}

function Chart({ data, color, fill, yFmt }: ChartProps) {
  const elRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    const el = elRef.current;
    if (!el) return;
    const opts: uPlot.Options = {
      width: el.clientWidth || 600,
      height: 140,
      cursor: { y: false, points: { size: 4 } },
      legend: { show: false },
      scales: { x: { time: true } },
      axes: [
        {
          stroke: "#8b949e",
          grid: { stroke: "#21262d", width: 1 },
          ticks: { stroke: "#21262d", width: 1 },
        },
        {
          stroke: "#8b949e",
          grid: { stroke: "#21262d", width: 1 },
          ticks: { stroke: "#21262d", width: 1 },
          size: 60,
          values: (_u, splits) => splits.map((v) => yFmt(v as number)),
        },
      ],
      series: [
        {},
        {
          stroke: color,
          width: 1.5,
          fill,
          points: { show: false },
          value: (_u, v) => (v == null ? "—" : yFmt(v)),
        },
      ],
    };
    const u = new uPlot(opts, data, el);
    plotRef.current = u;
    const ro = new ResizeObserver(() => {
      if (elRef.current) u.setSize({ width: elRef.current.clientWidth, height: 140 });
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      u.destroy();
      plotRef.current = null;
    };
  }, []);

  useEffect(() => {
    plotRef.current?.setData(data);
  }, [data]);

  return <div className="chart" ref={elRef} />;
}

interface Props {
  containerId: string;
  ncpu: number;
}

const EMPTY: uPlot.AlignedData = [[], []];

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
