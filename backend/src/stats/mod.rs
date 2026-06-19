mod cgroup;
mod dockerapi;
mod host;

pub use crate::types::ContainerRef;

use std::sync::Arc;
use std::time::Instant;

use bollard::Docker;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::registry::Registry;
use crate::store::StoreHandle;
use crate::types::StatsTick;

use cgroup::CgroupSource;
use dockerapi::DockerApiSource;
use host::HostBackend;

enum ContainerBackend {
    Cgroup {
        cgroup: CgroupSource,
        fallback: DockerApiSource,
    },
    Docker(DockerApiSource),
}

pub struct Collector {
    docker: Docker,
    host: HostBackend,
    backend: ContainerBackend,
    registry: Arc<Registry>,
    store: StoreHandle,
    tx: broadcast::Sender<Arc<StatsTick>>,
    cfg: Config,
}

impl Collector {
    pub async fn new(
        docker: Docker,
        registry: Arc<Registry>,
        store: StoreHandle,
        tx: broadcast::Sender<Arc<StatsTick>>,
        cfg: Config,
    ) -> Self {
        let host = HostBackend::detect(&docker).await;
        let backend = if !cfg.force_docker_stats && CgroupSource::available() {
            tracing::info!("stats backend: cgroup direct reads (docker-api fallback for unresolved containers)");
            ContainerBackend::Cgroup {
                cgroup: CgroupSource::new(),
                fallback: DockerApiSource::new(),
            }
        } else {
            tracing::info!("stats backend: docker api");
            ContainerBackend::Docker(DockerApiSource::new())
        };
        Collector {
            docker,
            host,
            backend,
            registry,
            store,
            tx,
            cfg,
        }
    }

    pub async fn run(mut self) {
        let mut ticker = tokio::time::interval(self.cfg.live_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_persist = Instant::now()
            .checked_sub(self.cfg.persist_interval)
            .unwrap_or_else(Instant::now);

        loop {
            ticker.tick().await;
            let refs = self.registry.running();
            let ncpu = self.host.ncpu();
            let mem_total = self.host.mem_total();

            let containers = match &mut self.backend {
                ContainerBackend::Cgroup { cgroup, fallback } => {
                    let mut stats = cgroup.sample(&refs, mem_total);
                    let resolved: std::collections::HashSet<&str> =
                        stats.iter().map(|s| s.id.as_str()).collect();
                    let missing: Vec<ContainerRef> = refs
                        .iter()
                        .filter(|r| !resolved.contains(r.id.as_str()))
                        .cloned()
                        .collect();
                    if !missing.is_empty() {
                        let extra =
                            fallback.sample(&self.docker, &missing, mem_total, ncpu).await;
                        stats.extend(extra);
                    }
                    stats
                }
                ContainerBackend::Docker(d) => {
                    d.sample(&self.docker, &refs, mem_total, ncpu).await
                }
            };
            let host = self.host.sample(&containers);

            let tick = Arc::new(StatsTick {
                ts: crate::util::now_millis(),
                host,
                containers,
            });

            let _ = self.tx.send(tick.clone());

            if last_persist.elapsed() >= self.cfg.persist_interval {
                last_persist = Instant::now();
                self.store.persist_tick(tick);
            }
        }
    }
}
