use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bollard::container::{LogOutput, LogsOptions};
use bollard::Docker;
use futures_util::StreamExt;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::util::{is_hex_id, short_id};

const BROADCAST_CAP: usize = 512;
const TAIL_READ_CAP: u64 = 512 * 1024;
const MAX_LINE_BYTES: usize = 256 * 1024;

struct Channel {
    tx: broadcast::Sender<String>,
    task: JoinHandle<()>,
}

pub struct LogManager {
    docker: Docker,
    dir: PathBuf,
    max_bytes: u64,
    keep: u32,
    backfill: usize,
    channels: Mutex<HashMap<String, Channel>>,
}

impl LogManager {
    pub async fn new(
        docker: Docker,
        dir: PathBuf,
        max_bytes: u64,
        keep: u32,
        backfill: usize,
    ) -> Arc<Self> {
        let _ = tokio::fs::create_dir_all(&dir).await;
        Arc::new(LogManager {
            docker,
            dir,
            max_bytes,
            keep,
            backfill,
            channels: Mutex::new(HashMap::new()),
        })
    }

    fn log_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.log", short_id(id)))
    }

    pub fn ensure(self: &Arc<Self>, id: &str) {
        if !is_hex_id(id) {
            return;
        }
        let mut guard = self.channels.lock().unwrap();
        if guard.contains_key(id) {
            return;
        }
        let (tx, _rx) = broadcast::channel::<String>(BROADCAST_CAP);
        let me = self.clone();
        let id_owned = id.to_string();
        let tx_task = tx.clone();
        let task = tokio::spawn(async move {
            me.stream(id_owned, tx_task).await;
        });
        guard.insert(id.to_string(), Channel { tx, task });
    }

    pub fn stop(&self, id: &str) {
        if let Some(ch) = self.channels.lock().unwrap().remove(id) {
            ch.task.abort();
        }
    }

    pub fn sync(self: &Arc<Self>, running_ids: &[String]) {
        let set: std::collections::HashSet<&str> = running_ids.iter().map(|s| s.as_str()).collect();
        let stale: Vec<String> = {
            let guard = self.channels.lock().unwrap();
            guard
                .keys()
                .filter(|k| !set.contains(k.as_str()))
                .cloned()
                .collect()
        };
        for id in stale {
            self.stop(&id);
        }
        for id in running_ids {
            self.ensure(id);
        }
    }

    pub fn subscribe(self: &Arc<Self>, id: &str) -> broadcast::Receiver<String> {
        self.ensure(id);
        let guard = self.channels.lock().unwrap();
        match guard.get(id) {
            Some(ch) => ch.tx.subscribe(),
            None => {
                let (tx, rx) = broadcast::channel(1);
                drop(tx);
                rx
            }
        }
    }

    pub async fn tail(&self, id: &str, n: usize) -> Vec<String> {
        if !is_hex_id(id) {
            return Vec::new();
        }
        let mut lines = read_tail(&self.log_path(id), n).await;
        if lines.len() < n {
            let rotated = self.dir.join(format!("{}.log.1", short_id(id)));
            let mut older = read_tail(&rotated, n - lines.len()).await;
            older.append(&mut lines);
            lines = older;
        }
        let len = lines.len();
        if len > n {
            lines.split_off(len - n)
        } else {
            lines
        }
    }

    pub async fn history(&self, id: &str, n: usize) -> Vec<String> {
        if !is_hex_id(id) || n == 0 {
            return Vec::new();
        }
        let opts = LogsOptions::<String> {
            follow: false,
            stdout: true,
            stderr: true,
            timestamps: true,
            tail: n.to_string(),
            ..Default::default()
        };
        let mut stream = std::pin::pin!(self.docker.logs(id, Some(opts)));
        let mut buf: Vec<u8> = Vec::new();
        let mut out: Vec<String> = Vec::new();
        while let Some(item) = stream.next().await {
            let bytes = match item {
                Ok(out) => message_bytes(out),
                Err(_) => break,
            };
            buf.extend_from_slice(&bytes);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                out.push(
                    String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1])
                        .trim_end_matches('\r')
                        .to_string(),
                );
            }
        }
        if !buf.is_empty() {
            out.push(String::from_utf8_lossy(&buf).trim_end_matches('\r').to_string());
        }
        if out.len() > n {
            out.drain(..out.len() - n);
        }
        out
    }

    async fn stream(self: Arc<Self>, id: String, tx: broadcast::Sender<String>) {
        let path = self.log_path(&id);
        let mut last_ts: Option<String> = read_last_timestamp(&path).await;
        loop {
            let mut writer = match RotatingWriter::open(path.clone(), self.max_bytes, self.keep).await {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("log file open failed for {}: {e}", short_id(&id));
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
            };
            let opts = LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                timestamps: true,
                tail: self.backfill.to_string(),
                ..Default::default()
            };
            let mut stream = std::pin::pin!(self.docker.logs(&id, Some(opts)));
            let mut buf: Vec<u8> = Vec::new();
            while let Some(item) = stream.next().await {
                let bytes = match item {
                    Ok(out) => message_bytes(out),
                    Err(_) => break,
                };
                buf.extend_from_slice(&bytes);
                while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                    let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1])
                        .trim_end_matches('\r')
                        .to_string();
                    if !accept_line(&line, &mut last_ts) {
                        continue;
                    }
                    if writer.write_line(&line).await.is_err() {
                        break;
                    }
                    let _ = tx.send(line);
                }
                if buf.len() > MAX_LINE_BYTES {
                    let line = String::from_utf8_lossy(&buf).trim_end_matches('\r').to_string();
                    buf.clear();
                    if writer.write_line(&line).await.is_err() {
                        break;
                    }
                    let _ = tx.send(line);
                }
                let _ = writer.flush().await;
            }
            let _ = writer.flush().await;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}

pub(crate) fn parse_ts(line: &str) -> Option<&str> {
    let head = line.split(' ').next()?;
    let b = head.as_bytes();
    if head.len() >= 20 && b.get(4) == Some(&b'-') && head.contains('T') {
        Some(head)
    } else {
        None
    }
}

fn accept_line(line: &str, last_ts: &mut Option<String>) -> bool {
    match (parse_ts(line), last_ts.as_deref()) {
        (Some(t), Some(prev)) => {
            if t <= prev {
                return false;
            }
            *last_ts = Some(t.to_string());
            true
        }
        (Some(t), None) => {
            *last_ts = Some(t.to_string());
            true
        }
        (None, _) => true,
    }
}

async fn read_last_timestamp(path: &Path) -> Option<String> {
    let lines = read_tail(path, 1).await;
    parse_ts(lines.last()?).map(|s| s.to_string())
}

fn message_bytes(out: LogOutput) -> bytes::Bytes {
    match out {
        LogOutput::StdOut { message } => message,
        LogOutput::StdErr { message } => message,
        LogOutput::Console { message } => message,
        LogOutput::StdIn { message } => message,
    }
}

struct RotatingWriter {
    path: PathBuf,
    file: BufWriter<File>,
    size: u64,
    max: u64,
    keep: u32,
}

impl RotatingWriter {
    async fn open(path: PathBuf, max: u64, keep: u32) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        let size = file.metadata().await.map(|m| m.len()).unwrap_or(0);
        Ok(RotatingWriter {
            path,
            file: BufWriter::new(file),
            size,
            max,
            keep,
        })
    }

    async fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.file.write_all(line.as_bytes()).await?;
        self.file.write_all(b"\n").await?;
        self.size += line.len() as u64 + 1;
        if self.max > 0 && self.size >= self.max {
            self.rotate().await?;
        }
        Ok(())
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush().await
    }

    async fn rotate(&mut self) -> std::io::Result<()> {
        self.file.flush().await?;
        let base = self.path.clone();
        let rotated = |i: u32| -> PathBuf {
            let mut p = base.clone();
            let name = format!("{}.{}", base.file_name().unwrap().to_string_lossy(), i);
            p.set_file_name(name);
            p
        };
        if self.keep == 0 {
            let f = OpenOptions::new().write(true).truncate(true).open(&self.path).await?;
            self.file = BufWriter::new(f);
            self.size = 0;
            return Ok(());
        }
        let _ = tokio::fs::remove_file(rotated(self.keep)).await;
        for i in (1..self.keep).rev() {
            let _ = tokio::fs::rename(rotated(i), rotated(i + 1)).await;
        }
        let _ = tokio::fs::rename(&self.path, rotated(1)).await;
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        self.file = BufWriter::new(f);
        self.size = 0;
        Ok(())
    }
}

async fn read_tail(path: &Path, n: usize) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL_READ_CAP);
    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return Vec::new();
    }
    let mut data = Vec::new();
    if file.read_to_end(&mut data).await.is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&data);
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let total = lines.len();
    if total > n {
        lines.split_off(total - n)
    } else {
        lines
    }
}
