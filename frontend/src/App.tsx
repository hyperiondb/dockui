import { useEffect, useMemo, useRef, useState } from "react";
import type { ContainerInfo, ContainerStat, HostStat } from "./types";
import { fetchContainers, openStatsStream } from "./api";
import { HostBar } from "./components/HostBar";
import { ContainerList } from "./components/ContainerList";
import { LogViewer } from "./components/LogViewer";
import { MetricsCharts } from "./components/MetricsCharts";
import { stateColor } from "./format";

const SPARK_LEN = 40;

export default function App() {
  const [containers, setContainers] = useState<ContainerInfo[]>([]);
  const [stats, setStats] = useState<Map<string, ContainerStat>>(new Map());
  const [cpuHist, setCpuHist] = useState<Map<string, number[]>>(new Map());
  const [host, setHost] = useState<HostStat | null>(null);
  const [connected, setConnected] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [showCharts, setShowCharts] = useState(true);

  const selectedRef = useRef(selectedId);
  selectedRef.current = selectedId;

  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const list = await fetchContainers();
        if (!alive) return;
        setContainers(list);
        if (!selectedRef.current) {
          const first = list.find((c) => c.state === "running") ?? list[0];
          if (first) setSelectedId(first.id);
        }
      } catch {
        return;
      }
    };
    load();
    const t = setInterval(load, 5000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);

  useEffect(() => {
    const es = openStatsStream((tick) => {
      setHost(tick.host);
      const m = new Map<string, ContainerStat>();
      for (const c of tick.containers) m.set(c.id, c);
      setStats(m);
      setCpuHist((prev) => {
        const next = new Map<string, number[]>();
        for (const c of tick.containers) {
          const arr = (prev.get(c.id) ?? []).concat(c.cpu_pct);
          next.set(c.id, arr.length > SPARK_LEN ? arr.slice(arr.length - SPARK_LEN) : arr);
        }
        return next;
      });
    });
    es.onopen = () => setConnected(true);
    es.onerror = () => setConnected(false);
    return () => es.close();
  }, []);

  const selected = useMemo(
    () => containers.find((c) => c.id === selectedId) ?? null,
    [containers, selectedId],
  );
  const running = containers.filter((c) => c.state === "running").length;
  const ncpu = host?.ncpu ?? 1;

  return (
    <div className="app">
      <HostBar host={host} running={running} total={containers.length} connected={connected} />
      <div className="body">
        <ContainerList
          containers={containers}
          stats={stats}
          cpuHistory={cpuHist}
          ncpu={ncpu}
          selectedId={selectedId}
          filter={filter}
          onSelect={setSelectedId}
          onFilter={setFilter}
        />
        <main className="main">
          {selected ? (
            <>
              <div className="cheader">
                <div className="ctitle">
                  <span className="dot" style={{ background: stateColor(selected.state) }} />
                  <span className="hname">{selected.name}</span>
                  <span className="himage">{selected.image}</span>
                </div>
                <div className="cright">
                  <span className="cstatus">{selected.status || selected.state}</span>
                  {selected.state === "running" && (
                    <button className={`btn ${showCharts ? "on" : ""}`} onClick={() => setShowCharts((s) => !s)}>
                      Charts
                    </button>
                  )}
                </div>
              </div>
              {showCharts && selected.state === "running" && (
                <MetricsCharts containerId={selected.id} ncpu={ncpu} />
              )}
              <LogViewer container={selected} />
            </>
          ) : (
            <div className="empty-pane">No container selected</div>
          )}
        </main>
      </div>
    </div>
  );
}
