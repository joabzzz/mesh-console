//! Inference bridge built on the public Rust SDK ([`mesh_api`]) from
//! <https://github.com/mesh-llm/mesh-llm> (pinned in `Cargo.toml`; transitive
//! `mesh_client`). Not the unrelated top-level `mesh-llm-client/` Dioxus app.
//!
//! mesh-console keeps using the management HTTP API on `:3131` for status,
//! catalog, and runtime control because the SDK does not expose those today.
//! For chat we go through `mesh_api::MeshClient`, pointing it at the local
//! node's OpenAI-compatible proxy port (the `api_port` field surfaced in
//! `MeshStatus` — typically 9337). This sidesteps the management proxy's
//! `503 LLM not ready` gate in
//! <https://github.com/mesh-llm/mesh-llm/blob/main/mesh-llm/src/api/routes/chat.rs>
//! and lets us debug inference end-to-end on this machine and through the
//! mesh, against the same HTTP surface third-party SDK users will hit.
//!
//! `MeshClient` owns a dedicated tokio runtime thread, so we build it inside
//! `spawn_blocking` and drop it the same way to avoid blocking the Dioxus
//! reactor on `thread.join()`.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use mesh_api::events::{Event, EventListener};
use mesh_api::{ChatMessage, ChatRequest, ClientBuilder, InviteToken, OwnerKeypair};
use tokio::sync::mpsc;

/// Validate an invite token using the SDK's parser. Returns the canonical
/// string form on success and a human-readable error on failure. The Home
/// view uses this to gate the Join button and surface bad tokens before we
/// fork a `mesh-llm serve --join …` subprocess.
pub fn validate_invite_token(raw: &str) -> Result<String, String> {
    let token = InviteToken::from_str(raw.trim()).map_err(|e| format!("invalid invite: {e}"))?;
    Ok(token.as_str().to_string())
}

/// Stable owner id derived from the persisted keypair. Useful as a "this
/// device" identifier in the Home view; mirrors what mesh-llm peers see.
pub fn owner_id() -> Result<String, String> {
    Ok(load_or_create_owner_keypair()?.owner_id())
}

const APP_DIR: &str = "mesh-console";
const KEYPAIR_FILE: &str = "owner.hex";
const SDK_API_BASE_ENV: &str = "MESH_CLIENT_API_BASE";

fn keypair_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|d| d.join(APP_DIR).join(KEYPAIR_FILE))
        .ok_or_else(|| "could not resolve user config directory".to_string())
}

/// Load the persisted owner keypair, or generate and save one on first run.
/// The file is plain hex (signing || encryption) at
/// `<config_dir>/mesh-console/owner.hex` so the same identity is reused across
/// app launches; mesh-llm peers see a stable owner_id.
pub fn load_or_create_owner_keypair() -> Result<OwnerKeypair, String> {
    let path = keypair_path()?;
    if let Ok(text) = std::fs::read_to_string(&path) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return OwnerKeypair::from_hex(trimmed);
        }
    }
    let kp = OwnerKeypair::generate();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create config dir {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, kp.to_hex()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(kp)
}

struct ChannelListener {
    tx: mpsc::UnboundedSender<Event>,
}

impl EventListener for ChannelListener {
    fn on_event(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}

/// One message in an SDK chat exchange (role + content).
pub type SdkMessage = (String, String);

/// Drive a single chat completion through the SDK and forward each token
/// delta to `on_delta`. Returns when the request completes or fails.
///
/// `token` is the invite token from `MeshStatus.token` (the mesh's join
/// secret). `api_port` is the local OpenAI proxy port from
/// `MeshStatus.api_port` (9337 by default).
pub async fn stream_chat(
    token: String,
    api_port: u16,
    model: String,
    messages: Vec<SdkMessage>,
    mut on_delta: impl FnMut(&str),
) -> Result<(), String> {
    if token.is_empty() {
        return Err(
            "no invite token in /api/status — start or join a mesh from the Network tab first"
                .into(),
        );
    }
    if api_port == 0 {
        return Err("mesh-llm has not reported an api_port yet".into());
    }

    let invite = InviteToken::from_str(&token).map_err(|e| format!("invalid invite token: {e}"))?;
    let owner = tokio::task::spawn_blocking(load_or_create_owner_keypair)
        .await
        .map_err(|e| format!("keypair task: {e}"))??;

    // mesh-api's ClientBuilder doesn't re-export `with_api_base_url`, so we
    // forward the proxy URL through the documented env var that
    // `mesh_client::ClientBuilder::new` reads (see
    // `mesh-llm/mesh-client/src/client/builder.rs`).
    std::env::set_var(SDK_API_BASE_ENV, format!("http://127.0.0.1:{api_port}"));

    let user_agent = format!("mesh-console/{}", env!("CARGO_PKG_VERSION"));
    let client = tokio::task::spawn_blocking(move || {
        ClientBuilder::new(owner, invite)
            .with_user_agent(user_agent)
            .with_connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("client build task: {e}"))??;

    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
    let listener: Arc<dyn EventListener> = Arc::new(ChannelListener { tx });

    let request = ChatRequest {
        model,
        messages: messages
            .into_iter()
            .map(|(role, content)| ChatMessage { role, content })
            .collect(),
    };
    let request_id = client.chat(request, listener).0;

    let mut received_anything = false;
    let outcome: Result<(), String> = loop {
        match rx.recv().await {
            Some(Event::TokenDelta {
                request_id: rid,
                delta,
            }) if rid == request_id => {
                if !delta.is_empty() {
                    received_anything = true;
                    on_delta(&delta);
                }
            }
            Some(Event::Completed { request_id: rid }) if rid == request_id => {
                break Ok(());
            }
            Some(Event::Failed {
                request_id: rid,
                error,
            }) if rid == request_id => {
                break Err(error);
            }
            Some(_) => {}
            None => break Err("SDK event channel closed before completion".into()),
        }
    };

    // CoreRuntime::drop joins the SDK's dedicated tokio thread, so dispose of
    // the client off the Dioxus async thread.
    let _ = tokio::task::spawn_blocking(move || drop(client)).await;

    if outcome.is_ok() && !received_anything {
        return Err(
            "SDK completed with no tokens — check that the local proxy is reachable on the api_port and that a model is loaded"
                .into(),
        );
    }
    outcome
}
