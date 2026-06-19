use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::stats::ContainerRef;
use crate::types::ContainerStat;

#[derive(Clone)]
enum CgPaths {
    V2 { base: PathBuf },
    V1 { cpu: PathBuf, mem: PathBuf },
}

pub struct CgroupSource {
    v2: bool,
    cache: HashMap<String, Option<CgPaths>>,
    prev: HashMap<String, (u64, Instant)>,
}

impl CgroupSource {
    pub fn available() -> bool {
        cfg!(target_os = "linux") && Path::new("/sys/fs/cgroup").exists()
    }

    pub fn new() -> Self {
        let v2 = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
        CgroupSource {
            v2,
            cache: HashMap::new(),
            prev: HashMap::new(),
        }
    }

    pub fn sample(&mut self, refs: &[ContainerRef], host_mem_total: u64) -> Vec<ContainerStat> {
        let now = Instant::now();
        let alive: std::collections::HashSet<&str> = refs.iter().map(|r| r.id.as_str()).collect();
        self.prev.retain(|id, _| alive.contains(id.as_str()));
        self.cache.retain(|id, _| alive.contains(id.as_str()));

        let mut out = Vec::with_capacity(refs.len());
        for r in refs {
            let paths = match self.resolve(&r.id) {
                Some(p) => p,
                None => continue,
            };
            let (usage_usec, mem_bytes, mem_limit) = match read_paths(&paths) {
                Some(v) => v,
                None => continue,
            };
            let cpu_pct = match self.prev.get(&r.id) {
                Some((prev_usage, prev_t)) => {
                    let dt_us = now.duration_since(*prev_t).as_micros() as u64;
                    let du = usage_usec.saturating_sub(*prev_usage);
                    if dt_us == 0 {
                        0.0
                    } else {
                        (du as f64 / dt_us as f64) * 100.0
                    }
                }
                None => 0.0,
            };
            self.prev.insert(r.id.clone(), (usage_usec, now));
            let limit = if mem_limit == 0 { host_mem_total } else { mem_limit };
            out.push(ContainerStat {
                id: r.id.clone(),
                cpu_pct: cpu_pct.max(0.0),
                mem_bytes,
                mem_limit: limit,
            });
        }
        out
    }

    fn resolve(&mut self, id: &str) -> Option<CgPaths> {
        if !self.cache.contains_key(id) {
            let found = if self.v2 { resolve_v2(id) } else { resolve_v1(id) };
            self.cache.insert(id.to_string(), found);
        }
        self.cache.get(id).cloned().flatten()
    }
}

fn resolve_v2(id: &str) -> Option<CgPaths> {
    let candidates = [
        format!("/sys/fs/cgroup/system.slice/docker-{id}.scope"),
        format!("/sys/fs/cgroup/docker/{id}"),
        format!("/sys/fs/cgroup/system.slice/docker-{id}.scope/init.scope"),
    ];
    for c in candidates {
        let base = PathBuf::from(&c);
        if base.join("cpu.stat").exists() {
            return Some(CgPaths::V2 { base });
        }
    }
    None
}

fn resolve_v1(id: &str) -> Option<CgPaths> {
    let cpu_candidates = [
        format!("/sys/fs/cgroup/cpu,cpuacct/docker/{id}"),
        format!("/sys/fs/cgroup/cpuacct/docker/{id}"),
        format!("/sys/fs/cgroup/cpu,cpuacct/system.slice/docker-{id}.scope"),
    ];
    let mem_candidates = [
        format!("/sys/fs/cgroup/memory/docker/{id}"),
        format!("/sys/fs/cgroup/memory/system.slice/docker-{id}.scope"),
    ];
    let cpu = cpu_candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.join("cpuacct.usage").exists())?;
    let mem = mem_candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.join("memory.usage_in_bytes").exists())?;
    Some(CgPaths::V1 { cpu, mem })
}

fn read_paths(paths: &CgPaths) -> Option<(u64, u64, u64)> {
    match paths {
        CgPaths::V2 { base } => {
            let usage_usec = read_field(&base.join("cpu.stat"), "usage_usec")?;
            let current = read_u64(&base.join("memory.current")).unwrap_or(0);
            let inactive = read_field(&base.join("memory.stat"), "inactive_file").unwrap_or(0);
            let mem = current.saturating_sub(inactive);
            let limit = read_max(&base.join("memory.max"));
            Some((usage_usec, mem, limit))
        }
        CgPaths::V1 { cpu, mem } => {
            let usage_ns = read_u64(&cpu.join("cpuacct.usage"))?;
            let usage_usec = usage_ns / 1000;
            let current = read_u64(&mem.join("memory.usage_in_bytes")).unwrap_or(0);
            let inactive =
                read_field(&mem.join("memory.stat"), "total_inactive_file").unwrap_or(0);
            let mem_bytes = current.saturating_sub(inactive);
            let raw_limit = read_u64(&mem.join("memory.limit_in_bytes")).unwrap_or(0);
            let limit = if raw_limit >= (1u64 << 62) { 0 } else { raw_limit };
            Some((usage_usec, mem_bytes, limit))
        }
    }
}

fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_max(path: &Path) -> u64 {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let s = s.trim();
            if s == "max" {
                0
            } else {
                s.parse().unwrap_or(0)
            }
        }
        Err(_) => 0,
    }
}

fn read_field(path: &Path, key: &str) -> Option<u64> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let mut it = line.split_whitespace();
        if it.next() == Some(key) {
            return it.next().and_then(|v| v.parse().ok());
        }
    }
    None
}
