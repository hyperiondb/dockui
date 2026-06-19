use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub created: i64,
}

#[derive(Clone, Debug)]
pub struct ContainerRef {
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContainerStat {
    pub id: String,
    pub cpu_pct: f64,
    pub mem_bytes: u64,
    pub mem_limit: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HostStat {
    pub cpu_pct: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub ncpu: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatsTick {
    pub ts: i64,
    pub host: HostStat,
    pub containers: Vec<ContainerStat>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContainerHistoryPoint {
    pub ts: i64,
    pub cpu_pct: f64,
    pub mem_bytes: i64,
    pub mem_limit: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HostHistoryPoint {
    pub ts: i64,
    pub cpu_pct: f64,
    pub mem_used: i64,
    pub mem_total: i64,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub range: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub tail: Option<usize>,
}

pub fn range_to_millis(range: &Option<String>) -> i64 {
    let r = range.as_deref().unwrap_or("1h");
    match r {
        "5m" => 5 * 60_000,
        "15m" => 15 * 60_000,
        "1h" => 60 * 60_000,
        "6h" => 6 * 60 * 60_000,
        "24h" | "1d" => 24 * 60 * 60_000,
        "7d" => 7 * 24 * 60 * 60_000,
        other => other
            .trim_end_matches('m')
            .parse::<i64>()
            .map(|m| m * 60_000)
            .unwrap_or(60 * 60_000),
    }
}
