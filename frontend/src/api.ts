import type {
  ContainerHistoryPoint,
  ContainerInfo,
  HostHistoryPoint,
  Range,
  StatsTick,
} from "./types";

async function getJSON<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json() as Promise<T>;
}

export function fetchContainers(): Promise<ContainerInfo[]> {
  return getJSON<ContainerInfo[]>("/api/containers");
}

export function fetchContainerHistory(
  id: string,
  range: Range,
): Promise<ContainerHistoryPoint[]> {
  return getJSON<ContainerHistoryPoint[]>(
    `/api/containers/${id}/history?range=${range}`,
  );
}

export function fetchHostHistory(range: Range): Promise<HostHistoryPoint[]> {
  return getJSON<HostHistoryPoint[]>(`/api/host/history?range=${range}`);
}

async function action(id: string, verb: string): Promise<void> {
  const res = await fetch(`/api/containers/${id}/${verb}`, { method: "POST" });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
}

export function startContainer(id: string): Promise<void> {
  return action(id, "start");
}

export function stopContainer(id: string): Promise<void> {
  return action(id, "stop");
}

export function restartContainer(id: string): Promise<void> {
  return action(id, "restart");
}

export function openStatsStream(onTick: (tick: StatsTick) => void): EventSource {
  const es = new EventSource("/api/stream/stats");
  es.onmessage = (e) => {
    try {
      onTick(JSON.parse(e.data) as StatsTick);
    } catch {
      return;
    }
  };
  return es;
}

export function openLogStream(
  id: string,
  tail: number,
  onLine: (line: string) => void,
): EventSource {
  const es = new EventSource(`/api/containers/${id}/logs?tail=${tail}`);
  es.onmessage = (e) => onLine(e.data);
  return es;
}
