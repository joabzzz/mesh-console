use crate::events::AppEvent;
use crate::state::{AppState, Mode, Phase, RunningInfo, Visibility};
use anyhow::{Context, Result};
use mesh_llm_sdk::MeshNode;
use mesh_llm_sdk::native_runtime::{NativeRuntimeInstallOptions, install_native_runtime};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// Tiny fast-start model (same default as mesh-app): ~500MB download, serves
/// in seconds. Used when join-and-share is requested without an explicit model.
pub const DEFAULT_MODEL: &str = "unsloth/Qwen3-0.6B-GGUF:Q4_K_M";

#[derive(Debug, Clone, Deserialize)]
pub struct HostRequest {
    pub model: String,
    pub visibility: Visibility,
    #[serde(default)]
    pub mesh_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JoinRequest {
    /// Invite token for a private mesh. Ignored when `public` is set.
    #[serde(default)]
    pub token: String,
    /// true = join THE global mesh via public discovery (no invite token);
    /// false = join a specific mesh by `token`.
    #[serde(default)]
    pub public: bool,
    /// true = also serve a model on this machine ("share this Mac's power"),
    /// false = chat-only client.
    #[serde(default)]
    pub share: bool,
    #[serde(default)]
    pub model: Option<String>,
}

pub async fn start_host(state: Arc<AppState>, req: HostRequest) {
    if let Err(err) = run_host(&state, req).await {
        state
            .set_phase(Phase::Error {
                message: format!("{err:#}"),
                recoverable: true,
            })
            .await;
    }
}

pub async fn start_join(state: Arc<AppState>, req: JoinRequest) {
    if let Err(err) = run_join(&state, req).await {
        state
            .set_phase(Phase::Error {
                message: format!("{err:#}"),
                recoverable: true,
            })
            .await;
    }
}

async fn run_host(state: &Arc<AppState>, req: HostRequest) -> Result<()> {
    ensure_runtime(state).await?;
    ensure_model(state, &req.model).await?;

    state
        .set_phase(Phase::Starting {
            mode: Mode::Host,
            model: Some(req.model.clone()),
        })
        .await;

    let mut builder = MeshNode::builder()
        .serve()
        .model(&req.model)
        .api_port(state.ports.api)
        // The console port still serves the node's management API (used by the
        // proxy, invite token, and serve/unserve). The embedded web console UI
        // is intentionally not served — this app has its own UI on :4640.
        .console_port(state.ports.console)
        .startup_timeout(Duration::from_secs(180));
    if let Some(name) = &req.mesh_name {
        builder = builder.mesh_name(name);
    }
    if req.visibility == Visibility::Public {
        builder = builder.publish(true);
    }

    let node = builder.start().await.context("starting mesh node")?;
    let invite_token = node.invite_token().map(str::to_owned);
    *state.node.lock().await = Some(node);

    state
        .set_phase(Phase::Running(RunningInfo {
            mode: Mode::Host,
            visibility: req.visibility,
            model: Some(req.model),
            serving: true,
            invite_token,
            api_port: state.ports.api,
            console_port: state.ports.console,
            mesh_name: req.mesh_name,
        }))
        .await;
    Ok(())
}

async fn run_join(state: &Arc<AppState>, mut req: JoinRequest) -> Result<()> {
    if req.share {
        // Sharing needs a model to serve; default to the tiny fast-start one
        // (same as mesh-app) so join-and-share never forces a model decision.
        if req.model.is_none() {
            req.model = Some(DEFAULT_MODEL.to_string());
        }
        ensure_runtime(state).await?;
        if let Some(model) = &req.model {
            ensure_model(state, model).await?;
        }
    }

    state
        .set_phase(Phase::Starting {
            mode: Mode::Join,
            model: req.model.clone(),
        })
        .await;

    let mut builder = if req.share {
        MeshNode::builder().serve()
    } else {
        MeshNode::builder().client()
    };
    if req.public {
        // Join the worldwide swarm via public discovery — no token. A
        // contributor also publishes so its served model is reachable.
        builder = builder.auto_join_public_mesh();
        if req.share {
            builder = builder.publish(true);
        }
    } else {
        builder = builder.join_token(req.token.trim());
    }
    builder = builder
        .api_port(state.ports.api)
        // Management API only; no embedded web console (see run_host).
        .console_port(state.ports.console)
        .startup_timeout(Duration::from_secs(180));
    if req.share
        && let Some(model) = &req.model
    {
        builder = builder.model(model);
    }

    let node = builder.start().await.context("joining mesh")?;
    let invite_token = node.invite_token().map(str::to_owned);
    *state.node.lock().await = Some(node);

    state
        .set_phase(Phase::Running(RunningInfo {
            mode: Mode::Join,
            // The global mesh is public by definition; a token-joined mesh's
            // visibility is the host's choice, so private is the safe display
            // default until /api/status says otherwise.
            visibility: if req.public {
                Visibility::Public
            } else {
                Visibility::Private
            },
            model: req.model,
            serving: req.share,
            invite_token,
            api_port: state.ports.api,
            console_port: state.ports.console,
            mesh_name: req.public.then(|| "Global mesh".to_string()),
        }))
        .await;
    Ok(())
}

pub async fn shutdown(state: &Arc<AppState>) -> Result<()> {
    if let Some(compute) = &state.compute {
        return compute.request_down().await;
    }
    // Agent first: cancel any in-flight reply and drop the handle before its
    // provider's node goes away. This does NOT reset the conversation — the
    // session pointer persists, so the next launch resumes the same chat.
    // (A deliberate "New chat" is what clears it; see agent::clear_session_pointer.)
    crate::agent::teardown(state).await;
    if let Some(node) = state.node.lock().await.take() {
        node.shutdown().await.context("shutting down mesh node")?;
    }
    state.set_phase(Phase::Idle).await;
    Ok(())
}

async fn ensure_runtime(state: &Arc<AppState>) -> Result<()> {
    state.set_phase(Phase::InstallingRuntime).await;
    let tx = state.events.clone();
    let progress = Arc::new(
        move |p: mesh_llm_sdk::native_runtime::NativeRuntimeDownloadProgress| {
            let _ = tx.send(AppEvent::DownloadProgress {
                kind: "runtime",
                label: p.native_runtime_id,
                file: None,
                downloaded_bytes: Some(p.downloaded_bytes),
                total_bytes: p.total_bytes,
                status: if p.finished { "done" } else { "downloading" },
                done: p.finished,
            });
        },
    );
    install_native_runtime(NativeRuntimeInstallOptions {
        progress: Some(progress),
        ..Default::default()
    })
    .await
    .context("installing native runtime")?;
    // Installing only places the dylibs in the cache. The CLI loads them via
    // initialize_host_runtime() before running; the embedded SDK path does
    // not, so we must load them into this process ourselves or skippy-ffi
    // panics with "native runtime library has not been loaded".
    mesh_llm_host_runtime::initialize_host_runtime()
        .await
        .context("loading native runtime libraries")?;
    Ok(())
}

async fn ensure_model(state: &Arc<AppState>, model: &str) -> Result<()> {
    let model_owned = model.to_string();
    let installed = tokio::task::spawn_blocking(move || {
        let cache = mesh_llm_node::models::default_huggingface_cache_dir();
        mesh_llm_node::models::scan_installed_models(cache)
            .iter()
            .any(|m| m.model_ref.contains(&model_owned))
    })
    .await
    .unwrap_or(false);
    if installed {
        return Ok(());
    }

    state
        .set_phase(Phase::Downloading {
            model: model.to_string(),
        })
        .await;
    // Byte-level progress arrives through the global OutputSink as
    // ModelDownloadProgress events; the phase only names the model.
    mesh_llm_host_runtime::models::download_model_ref_with_progress_details(model, true)
        .await
        .with_context(|| format!("downloading {model}"))?;
    Ok(())
}

/// Fetch the freshest invite token straight from the node's management API,
/// falling back to the one captured at startup.
pub async fn invite_token(state: &Arc<AppState>) -> Option<String> {
    let console_port = state.ports.console;
    let live = async {
        let status: serde_json::Value = state
            .http
            .get(format!("http://127.0.0.1:{console_port}/api/status"))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        status
            .get("token")
            .and_then(|t| t.as_str())
            .map(str::to_owned)
    }
    .await;
    if live.is_some() {
        return live;
    }
    match state.phase().await {
        Phase::Running(info) => info.invite_token,
        _ => None,
    }
}

/// Background task: keeps RunningInfo fresh from node events (e.g. an invite
/// token that materializes after startup).
pub fn spawn_event_listener(state: Arc<AppState>) {
    let mut rx = state.events.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let AppEvent::NodeEvent {
                event: "invite_token",
                detail,
            } = &event
                && let Some(token) = detail.get("token").and_then(|t| t.as_str())
            {
                let token = token.to_string();
                state
                    .update_running(|info| {
                        if info.invite_token.as_deref() != Some(token.as_str()) {
                            info.invite_token = Some(token);
                        }
                    })
                    .await;
            }
        }
    });
}
