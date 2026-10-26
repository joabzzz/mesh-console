use crate::state::{AppState, Phase};
use crate::{diagnose, node, proxy};
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde_json::json;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;

#[derive(rust_embed::Embed)]
#[folder = "../ui/dist"]
struct UiAssets;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/app/compute", get(crate::compute::status))
        .route("/app/compute/up", post(crate::compute::up))
        .route("/app/compute/down", post(crate::compute::down))
        .route("/app/state", get(app_state))
        .route("/app/events", get(app_events))
        .route("/app/diagnose", post(app_diagnose))
        .route("/app/host", post(app_host))
        .route("/app/join", post(app_join))
        .route("/app/invite", get(app_invite))
        .route("/app/pending_invite", get(app_pending_invite))
        .route("/app/chat", post(app_chat))
        .route("/app/sessions", get(app_sessions).post(app_create_session))
        .route("/app/sessions/{id}/activate", post(app_activate_session))
        .route("/app/sessions/{id}/archive", post(app_archive_session))
        .route("/app/sessions/{id}/history", get(app_session_history))
        .route("/app/history", get(app_history))
        .route("/app/new_chat", post(app_new_chat))
        .route("/app/shutdown", post(app_shutdown))
        .route("/app/reset", post(app_reset))
        .route("/api/{*path}", any(proxy::proxy))
        .route("/v1/{*path}", any(proxy::proxy))
        .fallback(get(ui_asset))
        .with_state(state)
}

pub async fn serve(state: Arc<AppState>, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
    node::spawn_event_listener(state.clone());
    axum::serve(listener, router(state)).await?;
    Ok(())
}

async fn app_state(State(state): State<Arc<AppState>>) -> Json<Phase> {
    Json(state.phase().await)
}

async fn app_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let current = state.phase().await;
    let first = futures_util::stream::once(async move {
        Ok(Event::default()
            .data(serde_json::to_string(&crate::events::AppEvent::Phase(current)).unwrap()))
    });
    let live = BroadcastStream::new(state.events.subscribe()).filter_map(|event| async move {
        let event = event.ok()?;
        let data = serde_json::to_string(&event).ok()?;
        Some(Ok(Event::default().data(data)))
    });
    Sse::new(first.chain(live)).keep_alive(KeepAlive::default())
}

async fn app_diagnose() -> Response {
    // hardware::survey shells out to system tools; keep it off the async runtime.
    match tokio::task::spawn_blocking(diagnose::diagnose).await {
        Ok(report) => Json(report).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{err:#}") })),
        )
            .into_response(),
    }
}

async fn app_host(
    State(state): State<Arc<AppState>>,
    Json(req): Json<node::HostRequest>,
) -> Response {
    if !ready_for_launch(&state).await {
        return busy_response(&state).await;
    }
    tokio::spawn(node::start_host(state.clone(), req));
    (StatusCode::ACCEPTED, Json(json!({ "ok": true }))).into_response()
}

async fn app_join(
    State(state): State<Arc<AppState>>,
    Json(req): Json<node::JoinRequest>,
) -> Response {
    if !req.public && req.token.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "empty_token" })),
        )
            .into_response();
    }
    if !ready_for_launch(&state).await {
        return busy_response(&state).await;
    }
    tokio::spawn(node::start_join(state.clone(), req));
    (StatusCode::ACCEPTED, Json(json!({ "ok": true }))).into_response()
}

async fn app_invite(State(state): State<Arc<AppState>>) -> Response {
    match node::invite_token(&state).await {
        Some(token) => Json(json!({
            "token": token,
            "approx_bytes": token.len(),
        }))
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no_invite_token" })),
        )
            .into_response(),
    }
}

/// Invite token from a `mesh://join/<token>` deep link that arrived before
/// the frontend's SSE connection was up. One-shot: reading takes the token,
/// so a later poll (or another window) won't replay it.
async fn app_pending_invite(State(state): State<Arc<AppState>>) -> Response {
    let token = state
        .pending_invite
        .lock()
        .expect("pending_invite lock")
        .take();
    Json(json!({ "token": token })).into_response()
}

#[derive(serde::Deserialize)]
struct ChatRequest {
    model: String,
    text: String,
    session_id: Option<String>,
}

/// One agent chat turn, streamed as SSE. The goose agent owns the
/// conversation history; the body carries only the new user message.
async fn app_chat(State(state): State<Arc<AppState>>, Json(req): Json<ChatRequest>) -> Response {
    if let Some(compute) = &state.compute {
        return crate::compute::chat(compute, &req.text).await;
    }
    use crate::agent::{self, Frame};
    use goose::agents::SessionConfig;
    use goose::conversation::message::Message;
    use tokio_stream::wrappers::ReceiverStream;

    if !matches!(state.phase().await, Phase::Running(_)) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "node_not_running" })),
        )
            .into_response();
    }

    let session_id = match req.session_id {
        Some(id) => id,
        None => match agent::active_or_create_session().await {
            Ok(session) => session.id,
            Err(err) => return session_error(err),
        },
    };
    // Agent selection and turn acquisition are atomic with session lifecycle
    // mutations. The owned guard rides in the streaming task.
    let (handle, turn_guard) = match agent::begin_turn(&state, &req.model, &session_id).await {
        Ok(prepared) => prepared,
        Err(err) => return session_error(err),
    };

    let cancel = handle.cancel_root.child_token();
    let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(64);

    // Drive the reply in its own task; the SSE body just drains the channel.
    // If the client disconnects, the receiver drops, sends fail, translation
    // returns, and we cancel the token so goose stops generating/tooling.
    let task_cancel = cancel.clone();
    tokio::spawn(async move {
        let _turn = turn_guard;
        let session_config = SessionConfig {
            id: handle.session_id.clone(),
            schedule_id: None,
            max_turns: Some(10),
            retry_config: None,
        };
        let user_message = Message::user().with_text(req.text);
        match handle
            .agent
            .reply(user_message, session_config, Some(task_cancel.clone()))
            .await
        {
            Ok(stream) => agent::translate_events(stream, tx).await,
            Err(err) => {
                let _ = tx.send(Frame::Error(format!("{err:#}"))).await;
                let _ = tx.send(Frame::Done).await;
            }
        }
        task_cancel.cancel();
    });

    Sse::new(
        ReceiverStream::new(rx).map(|frame| Ok::<_, Infallible>(crate::agent::frame_to_sse(frame))),
    )
    .keep_alive(KeepAlive::default())
    .into_response()
}

async fn app_sessions() -> Response {
    match crate::agent::list_sessions().await {
        Ok(sessions) => Json(sessions).into_response(),
        Err(err) => session_error(err),
    }
}

async fn app_create_session(State(state): State<Arc<AppState>>) -> Response {
    match crate::agent::create_session_while_idle(&state).await {
        Ok(session) => (StatusCode::CREATED, Json(session)).into_response(),
        Err(err) => session_error(err),
    }
}

async fn app_activate_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match crate::agent::activate_session(&state, &id).await {
        Ok(()) => Json(json!({ "ok": true, "session_id": id })).into_response(),
        Err(err) => session_error(err),
    }
}

#[derive(serde::Deserialize)]
struct ArchiveSessionRequest {
    #[serde(default = "default_true")]
    archived: bool,
}

const fn default_true() -> bool {
    true
}

async fn app_archive_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ArchiveSessionRequest>,
) -> Response {
    match crate::agent::archive_session(&state, &id, req.archived).await {
        Ok(()) => {
            Json(json!({ "ok": true, "session_id": id, "archived": req.archived })).into_response()
        }
        Err(err) => session_error(err),
    }
}

async fn app_session_history(Path(id): Path<String>) -> Response {
    match crate::agent::history_by_id(&id).await {
        Ok(history) => Json(history).into_response(),
        Err(err) => session_error(err),
    }
}

fn session_error(err: impl Into<crate::agent::SessionError>) -> Response {
    let err = err.into();
    let status = match &err {
        crate::agent::SessionError::Busy => StatusCode::CONFLICT,
        crate::agent::SessionError::NotFound => StatusCode::NOT_FOUND,
        crate::agent::SessionError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": err.to_string() }))).into_response()
}

/// The persisted conversation, shaped for the chat UI to repaint on launch.
/// Empty array when there's no ongoing chat (first run or just after reset).
/// Independent of node state — reading history doesn't need a running mesh.
async fn app_history(State(state): State<Arc<AppState>>) -> Response {
    if let Some(compute) = &state.compute {
        return crate::compute::history(compute).await;
    }
    Json(crate::agent::history().await).into_response()
}

/// "New chat": forget the remembered session so the next turn starts fresh,
/// and drop the live agent so it re-reads the (now absent) pointer. The old
/// conversation stays in goose's store, just no longer the active one.
async fn app_new_chat(State(state): State<Arc<AppState>>) -> Response {
    match crate::agent::create_session_while_idle(&state).await {
        Ok(session) => Json(json!({ "ok": true, "session_id": session.id })).into_response(),
        Err(err) => session_error(err),
    }
}

async fn app_shutdown(State(state): State<Arc<AppState>>) -> Response {
    match node::shutdown(&state).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{err:#}") })),
        )
            .into_response(),
    }
}

async fn app_reset(State(state): State<Arc<AppState>>) -> Response {
    crate::agent::teardown(&state).await;
    if matches!(state.phase().await, Phase::Error { .. }) {
        state.set_phase(Phase::Idle).await;
    }
    Json(json!({ "ok": true })).into_response()
}

async fn ready_for_launch(state: &Arc<AppState>) -> bool {
    if state.compute.is_some() {
        return false;
    }
    matches!(state.phase().await, Phase::Idle | Phase::Error { .. })
}

async fn busy_response(state: &Arc<AppState>) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "error": "busy", "phase": state.phase().await })),
    )
        .into_response()
}

/// Serves the built frontend. rust-embed reads ui/dist from disk in debug
/// builds and embeds it in release builds. SPA deep links fall back to
/// index.html.
async fn ui_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let asset = UiAssets::get(path).or_else(|| UiAssets::get("index.html"));
    match asset {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "ui not built").into_response(),
    }
}
