export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  created: number;
}

export interface ContainerStat {
  id: string;
  cpu_pct: number;
  mem_bytes: number;
  mem_limit: number;
}

export interface HostStat {
  cpu_pct: number;
  mem_used: number;
  mem_total: number;
  ncpu: number;
}

export interface StatsTick {
  ts: number;
  host: HostStat;
  containers: ContainerStat[];
}

export interface ContainerHistoryPoint {
  ts: number;
  cpu_pct: number;
  mem_bytes: number;
  mem_limit: number;
}

export interface HostHistoryPoint {
  ts: number;
  cpu_pct: number;
  mem_used: number;
  mem_total: number;
}

export type Range = "15m" | "1h" | "6h" | "24h" | "7d";
