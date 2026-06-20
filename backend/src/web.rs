use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bollard::Docker;
use futures_util::{Stream, StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::config::Config;
use crate::logs::LogManager;
use crate::registry::Registry;
use crate::store::StoreHandle;
use crate::types::{range_to_millis, HistoryQuery, LogQuery, StatsTick};
use crate::util::{is_hex_id, now_millis};

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<Registry>,
    pub store: StoreHandle,
    pub logs: Arc<LogManager>,
    pub stats_tx: broadcast::Sender<Arc<StatsTick>>,
    pub docker: Docker,
    pub cfg: Config,
}

#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../frontend/dist"]
struct Assets;

const TARGET_POINTS: i64 = 800;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/containers", get(list_containers))
        .route("/api/containers/:id/history", get(container_history))
        .route("/api/containers/:id/logs", get(container_logs))
        .route("/api/containers/:id/start", post(start_container))
        .route("/api/containers/:id/stop", post(stop_container))
        .route("/api/containers/:id/restart", post(restart_container))
        .route("/api/host/history", get(host_history))
        .route("/api/stream/stats", get(stream_stats))
        .fallback(static_handler)
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_containers(State(st): State<AppState>) -> impl IntoResponse {
    Json(st.registry.list())
}

async fn run_action<F, Fut>(st: &AppState, id: &str, action: F) -> Response
where
    F: FnOnce(Docker, String) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    if !is_hex_id(id) {
        return (StatusCode::BAD_REQUEST, "invalid id").into_response();
    }
    match action(st.docker.clone(), id.to_string()).await {
        Ok(()) => {
            st.registry.refresh(&st.docker).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn start_container(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    run_action(&st, &id, |d, id| async move {
        crate::docker::start_container(&d, &id).await
    })
    .await
}

async fn stop_container(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    run_action(&st, &id, |d, id| async move {
        crate::docker::stop_container(&d, &id).await
    })
    .await
}

async fn restart_container(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    run_action(&st, &id, |d, id| async move {
        crate::docker::restart_container(&d, &id).await
    })
    .await
}

fn window(range: &Option<String>, persist_ms: i64) -> (i64, i64) {
    let span = range_to_millis(range);
    let since = now_millis() - span;
    let bucket = (span / TARGET_POINTS).max(persist_ms).max(1);
    (since, bucket)
}

async fn host_history(
    State(st): State<AppState>,
    Query(q): Query<HistoryQuery>,
) -> impl IntoResponse {
    let persist = st.cfg.persist_interval.as_millis() as i64;
    let (since, bucket) = window(&q.range, persist);
    Json(st.store.host_history(since, bucket).await)
}

async fn container_history(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> impl IntoResponse {
    if !is_hex_id(&id) {
        return (StatusCode::BAD_REQUEST, "invalid id").into_response();
    }
    let persist = st.cfg.persist_interval.as_millis() as i64;
    let (since, bucket) = window(&q.range, persist);
    Json(st.store.container_history(id, since, bucket).await).into_response()
}

async fn stream_stats(
    State(st): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = st.stats_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(tick) => match Event::default().json_data(&*tick) {
                Ok(ev) => Some(Ok(ev)),
                Err(_) => None,
            },
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn container_logs(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<LogQuery>,
) -> Response {
    if !is_hex_id(&id) {
        return (StatusCode::BAD_REQUEST, "invalid id").into_response();
    }
    let tail = q.tail.unwrap_or(st.cfg.log_tail_default).min(5000);
    let mut history = st.logs.history(&id, tail).await;
    if history.is_empty() {
        history = st.logs.tail(&id, tail).await;
    }
    let last_ts = history
        .iter()
        .rev()
        .find_map(|l| crate::logs::parse_ts(l))
        .map(|s| s.to_string());
    let rx = st.logs.subscribe(&id);

    let hist_stream = futures_util::stream::iter(
        history
            .into_iter()
            .map(|l| Ok::<Event, Infallible>(Event::default().data(l))),
    );
    let live = BroadcastStream::new(rx).filter_map(move |res| {
        let last_ts = last_ts.clone();
        async move {
            match res {
                Ok(line) => {
                    if let (Some(prev), Some(t)) =
                        (last_ts.as_deref(), crate::logs::parse_ts(&line))
                    {
                        if t <= prev {
                            return None;
                        }
                    }
                    Some(Ok::<Event, Infallible>(Event::default().data(line)))
                }
                Err(_) => None,
            }
        }
    });
    let combined = hist_stream.chain(live);
    Sse::new(combined)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(header::CONTENT_TYPE, mime.as_ref())],
            content.data.into_owned(),
        )
            .into_response();
    }

    match Assets::get("index.html") {
        Some(content) => (
            [(header::CONTENT_TYPE, "text/html")],
            content.data.into_owned(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
