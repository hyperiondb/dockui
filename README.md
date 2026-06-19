# dockui

A minimal, low-overhead Docker dashboard for servers — container list, live logs, and
CPU/memory (per container **and** whole host) with history. Think Dozzle, but built to
sip resources.

- **Rust backend** (axum + tokio, 2 worker threads), **React frontend** (Vite + uPlot),
  shipped as a **single static binary** with the UI embedded.
- **Stats the cheap way:** reads cgroup counters straight from the filesystem instead of
  Docker's expensive `/stats` stream. A few tiny file reads every 2 s — not a per-container
  streaming HTTP connection.
- **History that survives restarts:** samples are downsampled into embedded **SQLite**
  (WAL) with automatic **7-day** retention and pruning.
- **Logs never lost:** when a container is first watched, dockui backfills its recent
  history, then streams new output into rotating files on disk and fans it out live to the
  browser over SSE (timestamp-deduplicated so restarts don't double up).
- **Read-only:** lists, logs, and metrics only. No start/stop, no auth — put it behind your
  VPN / reverse proxy.

---

## Quick start (Docker Compose)

```bash
docker compose up -d --build
# open http://localhost:8080
```

The compose file mounts three things:

| Mount | Why |
|---|---|
| `/var/run/docker.sock:ro` | list containers, read events, stream logs |
| `/sys/fs/cgroup:ro` | direct CPU/memory counters (the low-overhead path) |
| `dockui-data:/data` | SQLite history + streamed log files |

> On **Linux** the cgroup mount enables the zero-cost stats path. On **Docker
> Desktop (macOS/Windows)** there is no host cgroup tree, so dockui automatically
> falls back to the Docker stats API — everything still works, just slightly less cheap.

### Run without compose

```bash
docker run -d --name dockui -p 8080:8080 \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  -v /sys/fs/cgroup:/sys/fs/cgroup:ro \
  -v dockui-data:/data \
  dockui:latest
```

---

## Configuration

All via environment variables (defaults shown):

| Variable | Default | Description |
|---|---|---|
| `DOCKUI_BIND` | `0.0.0.0:8080` | listen address |
| `DOCKUI_DATA_DIR` | `./data` (`/data` in image) | SQLite DB + log files |
| `DOCKUI_LIVE_INTERVAL_MS` | `2000` | live sampling / broadcast interval |
| `DOCKUI_PERSIST_INTERVAL_MS` | `15000` | how often samples are written to SQLite |
| `DOCKUI_RETENTION_DAYS` | `7` | history retention |
| `DOCKUI_PRUNE_INTERVAL_SECS` | `3600` | how often old rows are pruned |
| `DOCKUI_LOG_MAX_BYTES` | `10485760` | per-container log file size before rotation |
| `DOCKUI_LOG_KEEP` | `3` | rotated log files to keep |
| `DOCKUI_LOG_TAIL_DEFAULT` | `500` | lines sent when opening a log view |
| `DOCKUI_LOG_BACKFILL` | `200` | recent lines pulled from a container the first time it's watched |
| `DOCKUI_FORCE_DOCKER_STATS` | `false` | force the Docker stats API even on Linux |
| `RUST_LOG` | `info` | log verbosity (`tracing` filter) |

---

## Why it's cheap

Dozzle and similar tools usually open a streaming `GET /containers/{id}/stats` connection
per container; Docker computes those stats and the constant HTTP framing adds up to a few
percent of a core. dockui instead:

1. **Reads cgroup files directly.** Per tick it reads `cpu.stat`, `memory.current`, and
   `memory.stat` (cgroup v2; v1 supported too) — microsecond-cheap, no daemon round-trip.
2. **Decouples live from persisted.** It broadcasts to the UI every 2 s but writes to
   SQLite every 15 s, and history queries are **bucket-aggregated** server-side so a 7-day
   chart returns a few hundred points, not tens of thousands.
3. **Streams logs once.** One follow stream per container is multiplexed to the disk file
   and all connected browsers; the browser batches DOM updates every 150 ms.

Host CPU/memory come from `/proc/stat` and `/proc/meminfo` (not namespaced, so a container
sees the real host).

---

## How CPU% is shown

The UI shows each container's CPU as a share of the **whole host** (0–100%, the same scale as
the host CPU bar), so the numbers are directly comparable and roughly add up to host usage.
Hover a container's CPU to also see the classic `docker stats` value where **100% = one core**.

> Example: `40%` of one core on a 6-core host is shown as `40 ÷ 6 ≈ 6.7%` of the host.

The JSON API (`/api/.../history`, `/api/stream/stats`) reports the raw per-core value
(`100 = 1 core`); the host-relative conversion happens in the UI.

## Troubleshooting

- **A container shows no logs.** dockui can only show what Docker itself has. Check
  `docker logs <name>` — if that's empty, the app isn't writing to stdout/stderr (it may log
  to a file inside the container), or the container uses a logging driver other than
  `json-file`/`local` (which `docker logs` can't read). That's a container-side setting, not
  a dockui limitation. When Docker does have output, dockui backfills the last
  `DOCKUI_LOG_BACKFILL` lines on open.

---

## Architecture

```
backend/
  src/
    main.rs        wiring: registry watcher, collector, pruner, axum server
    config.rs      env config
    types.rs       shared serde types
    docker.rs      bollard: list containers, container events
    registry.rs    in-memory container list (events + 15s refresh)
    stats/
      mod.rs       collector loop: sample -> broadcast -> persist
      cgroup.rs    cgroup v2/v1 reader (Linux)
      dockerapi.rs Docker stats fallback (dev / non-Linux)
      host.rs      /proc host stats (+ docker info fallback)
    store.rs       SQLite actor (own thread): insert, bucketed history, prune
    logs.rs        per-container log streams -> rotating files + broadcast + tail
    web.rs         REST + SSE + embedded SPA
frontend/          React + Vite + uPlot, built into backend via rust-embed
```

### HTTP API

| Method | Path | Description |
|---|---|---|
| GET | `/api/containers` | container list |
| GET | `/api/containers/:id/history?range=1h` | per-container CPU/mem history |
| GET | `/api/containers/:id/logs?tail=500` | SSE: log tail then live lines |
| GET | `/api/host/history?range=1h` | host CPU/mem history |
| GET | `/api/stream/stats` | SSE: live host + all-container stats |
| GET | `/api/health` | health check |

`range` accepts `15m`, `1h`, `6h`, `24h`, `7d`.

---

## Development

```bash
# backend (terminal 1) — needs a reachable Docker socket
cd backend && cargo run

# frontend (terminal 2) — Vite dev server proxies /api to :8080
cd frontend && npm install && npm run dev
# open http://localhost:5173
```

Production build (single binary):

```bash
cd frontend && npm install && npm run build   # emits frontend/dist
cd ../backend && cargo build --release        # embeds dist into the binary
./target/release/dockui
```

> Building the backend needs a C compiler (for bundled SQLite). The provided Dockerfile
> handles this for you; for local native builds use a Linux/macOS host or MSVC on Windows.

## License

MIT
