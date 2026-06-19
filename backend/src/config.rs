use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub live_interval: Duration,
    pub persist_interval: Duration,
    pub retention: Duration,
    pub prune_interval: Duration,
    pub log_max_bytes: u64,
    pub log_keep: u32,
    pub log_tail_default: usize,
    pub log_backfill: usize,
    pub force_docker_stats: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            bind: env_string("DOCKUI_BIND", "0.0.0.0:8080"),
            data_dir: PathBuf::from(env_string("DOCKUI_DATA_DIR", "./data")),
            live_interval: Duration::from_millis(env_u64("DOCKUI_LIVE_INTERVAL_MS", 2000)),
            persist_interval: Duration::from_millis(env_u64("DOCKUI_PERSIST_INTERVAL_MS", 15000)),
            retention: Duration::from_secs(env_u64("DOCKUI_RETENTION_DAYS", 7) * 86400),
            prune_interval: Duration::from_secs(env_u64("DOCKUI_PRUNE_INTERVAL_SECS", 3600)),
            log_max_bytes: env_u64("DOCKUI_LOG_MAX_BYTES", 10 * 1024 * 1024),
            log_keep: env_u64("DOCKUI_LOG_KEEP", 3) as u32,
            log_tail_default: env_u64("DOCKUI_LOG_TAIL_DEFAULT", 500) as usize,
            log_backfill: env_u64("DOCKUI_LOG_BACKFILL", 200) as usize,
            force_docker_stats: env_bool("DOCKUI_FORCE_DOCKER_STATS", false),
        }
    }
}

fn env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}
