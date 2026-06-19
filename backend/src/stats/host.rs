use bollard::Docker;

use crate::types::{ContainerStat, HostStat};

pub enum HostBackend {
    Proc(ProcHost),
    Fallback(FallbackHost),
}

impl HostBackend {
    pub async fn detect(docker: &Docker) -> Self {
        if cfg!(target_os = "linux") && std::path::Path::new("/proc/stat").exists() {
            HostBackend::Proc(ProcHost::default())
        } else {
            HostBackend::Fallback(FallbackHost::from_docker(docker).await)
        }
    }

    pub fn ncpu(&self) -> usize {
        match self {
            HostBackend::Proc(p) => p.ncpu.max(1),
            HostBackend::Fallback(f) => f.ncpu.max(1),
        }
    }

    pub fn mem_total(&self) -> u64 {
        match self {
            HostBackend::Proc(p) => p.mem_total,
            HostBackend::Fallback(f) => f.mem_total,
        }
    }

    pub fn sample(&mut self, containers: &[ContainerStat]) -> HostStat {
        match self {
            HostBackend::Proc(p) => p.sample(),
            HostBackend::Fallback(f) => f.sample(containers),
        }
    }
}

#[derive(Default)]
pub struct ProcHost {
    prev: Option<(u64, u64)>,
    ncpu: usize,
    mem_total: u64,
}

impl ProcHost {
    fn refresh_static(&mut self) {
        if self.ncpu == 0 {
            self.ncpu = read_ncpu();
        }
        let (total, _avail) = read_meminfo();
        self.mem_total = total;
    }

    pub fn sample(&mut self) -> HostStat {
        self.refresh_static();
        let cpu_pct = self.cpu_pct();
        let (total, avail) = read_meminfo();
        HostStat {
            cpu_pct,
            mem_used: total.saturating_sub(avail),
            mem_total: total,
            ncpu: self.ncpu.max(1),
        }
    }

    fn cpu_pct(&mut self) -> f64 {
        let stat = match std::fs::read_to_string("/proc/stat") {
            Ok(s) => s,
            Err(_) => return 0.0,
        };
        let line = match stat.lines().next() {
            Some(l) if l.starts_with("cpu ") || l.starts_with("cpu\t") => l,
            _ => return 0.0,
        };
        let nums: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|n| n.parse().ok())
            .collect();
        if nums.len() < 4 {
            return 0.0;
        }
        let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
        let total: u64 = nums.iter().sum();
        let busy = total.saturating_sub(idle);
        let pct = match self.prev {
            Some((pbusy, ptotal)) => {
                let dt = total.saturating_sub(ptotal);
                let db = busy.saturating_sub(pbusy);
                if dt == 0 {
                    0.0
                } else {
                    (db as f64 / dt as f64) * 100.0
                }
            }
            None => 0.0,
        };
        self.prev = Some((busy, total));
        pct.clamp(0.0, 100.0)
    }
}

fn read_ncpu() -> usize {
    std::fs::read_to_string("/proc/stat")
        .map(|s| {
            s.lines()
                .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).map_or(false, |b| b.is_ascii_digit()))
                .count()
        })
        .unwrap_or(0)
        .max(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
}

fn read_meminfo() -> (u64, u64) {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    let mut total = 0u64;
    let mut avail = 0u64;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail = parse_kb(rest);
        }
    }
    (total, avail.min(total))
}

fn parse_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .map(|kb| kb * 1024)
        .unwrap_or(0)
}

pub struct FallbackHost {
    pub ncpu: usize,
    pub mem_total: u64,
}

impl FallbackHost {
    pub async fn from_docker(docker: &Docker) -> Self {
        let (ncpu, mem_total) = match docker.info().await {
            Ok(info) => (
                info.ncpu.unwrap_or(1).max(1) as usize,
                info.mem_total.unwrap_or(0).max(0) as u64,
            ),
            Err(_) => (
                std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
                0,
            ),
        };
        FallbackHost { ncpu, mem_total }
    }

    pub fn sample(&mut self, containers: &[ContainerStat]) -> HostStat {
        let sum_cpu: f64 = containers.iter().map(|c| c.cpu_pct).sum();
        let sum_mem: u64 = containers.iter().map(|c| c.mem_bytes).sum();
        let cpu_pct = (sum_cpu / self.ncpu.max(1) as f64).clamp(0.0, 100.0);
        HostStat {
            cpu_pct,
            mem_used: sum_mem.min(self.mem_total).max(0),
            mem_total: self.mem_total,
            ncpu: self.ncpu.max(1),
        }
    }
}
