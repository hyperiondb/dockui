use std::collections::HashMap;

use bollard::container::StatsOptions;
use bollard::Docker;
use futures_util::StreamExt;

use crate::stats::ContainerRef;
use crate::types::ContainerStat;

#[derive(Default)]
pub struct DockerApiSource {
    prev: HashMap<String, (u64, u64)>,
}

impl DockerApiSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn sample(
        &mut self,
        docker: &Docker,
        refs: &[ContainerRef],
        host_mem_total: u64,
        ncpu: usize,
    ) -> Vec<ContainerStat> {
        let alive: std::collections::HashSet<&str> = refs.iter().map(|r| r.id.as_str()).collect();
        self.prev.retain(|id, _| alive.contains(id.as_str()));

        let futures = refs.iter().map(|r| fetch_one(docker, r.id.clone()));
        let raw = futures_util::future::join_all(futures).await;

        let mut out = Vec::with_capacity(refs.len());
        for sample in raw.into_iter().flatten() {
            let Sample {
                id,
                total_usage,
                system_usage,
                online_cpus,
                mem_bytes,
                mem_limit,
            } = sample;
            let cpus = if online_cpus > 0 { online_cpus } else { ncpu as u64 };
            let cpu_pct = match self.prev.get(&id) {
                Some((p_total, p_system)) => {
                    let cpu_delta = total_usage.saturating_sub(*p_total) as f64;
                    let sys_delta = system_usage.saturating_sub(*p_system) as f64;
                    if sys_delta > 0.0 && cpu_delta > 0.0 {
                        (cpu_delta / sys_delta) * cpus as f64 * 100.0
                    } else {
                        0.0
                    }
                }
                None => 0.0,
            };
            self.prev.insert(id.clone(), (total_usage, system_usage));
            let limit = if mem_limit == 0
                || (host_mem_total > 0 && mem_limit >= host_mem_total.saturating_mul(4))
            {
                host_mem_total
            } else {
                mem_limit
            };
            out.push(ContainerStat {
                id,
                cpu_pct: cpu_pct.max(0.0),
                mem_bytes,
                mem_limit: limit,
            });
        }
        out
    }
}

struct Sample {
    id: String,
    total_usage: u64,
    system_usage: u64,
    online_cpus: u64,
    mem_bytes: u64,
    mem_limit: u64,
}

async fn fetch_one(docker: &Docker, id: String) -> Option<Sample> {
    let opts = StatsOptions {
        stream: false,
        one_shot: true,
    };
    let mut stream = std::pin::pin!(docker.stats(&id, Some(opts)));
    let stats = stream.next().await?.ok()?;

    let cpu = stats.cpu_stats;
    let total_usage = cpu.cpu_usage.total_usage;
    let system_usage = cpu.system_cpu_usage.unwrap_or(0);
    let online_cpus = cpu.online_cpus.unwrap_or(0);

    let mem = stats.memory_stats;
    let mem_bytes = mem.usage.unwrap_or(0);
    let mem_limit = mem.limit.unwrap_or(0);

    Some(Sample {
        id,
        total_usage,
        system_usage,
        online_cpus,
        mem_bytes,
        mem_limit,
    })
}
