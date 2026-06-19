import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

interface ChartProps {
  data: uPlot.AlignedData;
  color: string;
  fill: string;
  yFmt: (v: number) => string;
  height?: number;
}

export function Chart({ data, color, fill, yFmt, height = 140 }: ChartProps) {
  const elRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    const el = elRef.current;
    if (!el) return;
    const opts: uPlot.Options = {
      width: el.clientWidth || 600,
      height,
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
      if (elRef.current) u.setSize({ width: elRef.current.clientWidth, height });
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
