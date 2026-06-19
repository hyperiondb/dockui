use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tokio::sync::oneshot;

use crate::types::{ContainerHistoryPoint, HostHistoryPoint, StatsTick};

enum Cmd {
    Persist(Arc<StatsTick>),
    ContainerHistory {
        id: String,
        since: i64,
        bucket: i64,
        reply: oneshot::Sender<Vec<ContainerHistoryPoint>>,
    },
    HostHistory {
        since: i64,
        bucket: i64,
        reply: oneshot::Sender<Vec<HostHistoryPoint>>,
    },
    Prune {
        older_than: i64,
    },
}

#[derive(Clone)]
pub struct StoreHandle {
    tx: mpsc::Sender<Cmd>,
}

impl StoreHandle {
    pub fn open(db_path: &Path) -> Result<StoreHandle> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("opening sqlite db at {}", db_path.display()))?;
        init_db(&conn)?;
        let (tx, rx) = mpsc::channel::<Cmd>();
        thread::Builder::new()
            .name("dockui-store".into())
            .spawn(move || store_loop(conn, rx))
            .context("spawning store thread")?;
        Ok(StoreHandle { tx })
    }

    pub fn persist_tick(&self, tick: Arc<StatsTick>) {
        let _ = self.tx.send(Cmd::Persist(tick));
    }

    pub async fn container_history(
        &self,
        id: String,
        since: i64,
        bucket: i64,
    ) -> Vec<ContainerHistoryPoint> {
        let (reply, rx) = oneshot::channel();
        if self
            .tx
            .send(Cmd::ContainerHistory {
                id,
                since,
                bucket: bucket.max(1),
                reply,
            })
            .is_err()
        {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }

    pub async fn host_history(&self, since: i64, bucket: i64) -> Vec<HostHistoryPoint> {
        let (reply, rx) = oneshot::channel();
        if self
            .tx
            .send(Cmd::HostHistory {
                since,
                bucket: bucket.max(1),
                reply,
            })
            .is_err()
        {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }

    pub fn prune(&self, older_than: i64) {
        let _ = self.tx.send(Cmd::Prune { older_than });
    }
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = OFF;
         CREATE TABLE IF NOT EXISTS container_stats (
             ts           INTEGER NOT NULL,
             container_id TEXT    NOT NULL,
             cpu_pct      REAL    NOT NULL,
             mem_bytes    INTEGER NOT NULL,
             mem_limit    INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_cs_id_ts ON container_stats (container_id, ts);
         CREATE INDEX IF NOT EXISTS idx_cs_ts ON container_stats (ts);
         CREATE TABLE IF NOT EXISTS host_stats (
             ts        INTEGER NOT NULL,
             cpu_pct   REAL    NOT NULL,
             mem_used  INTEGER NOT NULL,
             mem_total INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_hs_ts ON host_stats (ts);",
    )
    .context("initializing schema")?;
    Ok(())
}

fn store_loop(mut conn: Connection, rx: mpsc::Receiver<Cmd>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Persist(tick) => {
                if let Err(e) = persist(&mut conn, &tick) {
                    tracing::warn!("persist failed: {e}");
                }
            }
            Cmd::ContainerHistory {
                id,
                since,
                bucket,
                reply,
            } => {
                let rows = query_container(&conn, &id, since, bucket).unwrap_or_default();
                let _ = reply.send(rows);
            }
            Cmd::HostHistory {
                since,
                bucket,
                reply,
            } => {
                let rows = query_host(&conn, since, bucket).unwrap_or_default();
                let _ = reply.send(rows);
            }
            Cmd::Prune { older_than } => {
                if let Err(e) = prune(&conn, older_than) {
                    tracing::warn!("prune failed: {e}");
                }
            }
        }
    }
}

fn persist(conn: &mut Connection, tick: &StatsTick) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO host_stats (ts, cpu_pct, mem_used, mem_total) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tick.ts, tick.host.cpu_pct, tick.host.mem_used as i64, tick.host.mem_total as i64],
    )?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO container_stats (ts, container_id, cpu_pct, mem_bytes, mem_limit) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for c in &tick.containers {
            stmt.execute(rusqlite::params![
                tick.ts,
                c.id,
                c.cpu_pct,
                c.mem_bytes as i64,
                c.mem_limit as i64
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn query_container(
    conn: &Connection,
    id: &str,
    since: i64,
    bucket: i64,
) -> Result<Vec<ContainerHistoryPoint>> {
    let mut stmt = conn.prepare_cached(
        "SELECT (ts / ?3) * ?3 AS b, AVG(cpu_pct), AVG(mem_bytes), MAX(mem_limit)
         FROM container_stats
         WHERE container_id = ?1 AND ts >= ?2
         GROUP BY b
         ORDER BY b ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![id, since, bucket], |r| {
            Ok(ContainerHistoryPoint {
                ts: r.get(0)?,
                cpu_pct: r.get(1)?,
                mem_bytes: r.get::<_, f64>(2)? as i64,
                mem_limit: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn query_host(conn: &Connection, since: i64, bucket: i64) -> Result<Vec<HostHistoryPoint>> {
    let mut stmt = conn.prepare_cached(
        "SELECT (ts / ?2) * ?2 AS b, AVG(cpu_pct), AVG(mem_used), MAX(mem_total)
         FROM host_stats
         WHERE ts >= ?1
         GROUP BY b
         ORDER BY b ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![since, bucket], |r| {
            Ok(HostHistoryPoint {
                ts: r.get(0)?,
                cpu_pct: r.get(1)?,
                mem_used: r.get::<_, f64>(2)? as i64,
                mem_total: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn prune(conn: &Connection, older_than: i64) -> Result<()> {
    let a = conn.execute("DELETE FROM container_stats WHERE ts < ?1", [older_than])?;
    let b = conn.execute("DELETE FROM host_stats WHERE ts < ?1", [older_than])?;
    if a + b > 0 {
        tracing::debug!("pruned {} container + {} host rows", a, b);
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
    }
    Ok(())
}
