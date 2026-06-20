mod config;
mod docker;
mod logs;
mod registry;
mod stats;
mod store;
mod types;
mod util;
mod web;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bollard::Docker;
use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::logs::LogManager;
use crate::registry::Registry;
use crate::stats::Collector;
use crate::store::StoreHandle;
use crate::types::StatsTick;
use crate::web::AppState;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,dockui=info")),
        )
        .compact()
        .init();

    let cfg = Config::from_env();
    tokio::fs::create_dir_all(&cfg.data_dir)
        .await
        .with_context(|| format!("creating data dir {}", cfg.data_dir.display()))?;

    let docker = docker::connect().context("connecting to docker (is the socket mounted?)")?;
    match docker.version().await {
        Ok(v) => tracing::info!(
            "connected to docker {} (api {})",
            v.version.unwrap_or_default(),
            v.api_version.unwrap_or_default()
        ),
        Err(e) => tracing::warn!("docker not reachable yet: {e}"),
    }

    let store = StoreHandle::open(&cfg.data_dir.join("dockui.db")).context("opening store")?;
    let logs = LogManager::new(
        docker.clone(),
        cfg.data_dir.join("logs"),
        cfg.log_max_bytes,
        cfg.log_keep,
        cfg.log_backfill,
    )
    .await;

    let registry = Arc::new(Registry::new());
    registry.refresh(&docker).await;
    logs.sync(&running_ids(&registry));

    let (stats_tx, _rx) = broadcast::channel::<Arc<StatsTick>>(64);

    tokio::spawn(registry_watch(docker.clone(), registry.clone(), logs.clone()));
    tokio::spawn(periodic_refresh(
        docker.clone(),
        registry.clone(),
        logs.clone(),
    ));

    let collector = Collector::new(
        docker.clone(),
        registry.clone(),
        store.clone(),
        stats_tx.clone(),
        cfg.clone(),
    )
    .await;
    tokio::spawn(collector.run());

    tokio::spawn(prune_task(store.clone(), cfg.clone()));

    let state = AppState {
        registry,
        store,
        logs,
        stats_tx,
        docker,
        cfg: cfg.clone(),
    };
    let app = web::router(state);

    let listener = TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    tracing::info!("dockui listening on http://{}", cfg.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

fn running_ids(registry: &Registry) -> Vec<String> {
    registry.running().into_iter().map(|r| r.id).collect()
}

async fn registry_watch(docker: Docker, registry: Arc<Registry>, logs: Arc<LogManager>) {
    loop {
        let mut stream = std::pin::pin!(docker::container_events(&docker));
        while let Some(ev) = stream.next().await {
            match ev {
                Ok(ev) => {
                    tracing::debug!("container event: {} {}", ev.action, util::short_id(&ev.id));
                    registry.refresh(&docker).await;
                    logs.sync(&running_ids(&registry));
                }
                Err(e) => {
                    tracing::warn!("event stream error: {e}");
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn periodic_refresh(docker: Docker, registry: Arc<Registry>, logs: Arc<LogManager>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(15));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        registry.refresh(&docker).await;
        logs.sync(&running_ids(&registry));
    }
}

async fn prune_task(store: StoreHandle, cfg: Config) {
    let mut ticker = tokio::time::interval(cfg.prune_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let retention_ms = cfg.retention.as_millis() as i64;
    loop {
        ticker.tick().await;
        store.prune(util::now_millis() - retention_ms);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
