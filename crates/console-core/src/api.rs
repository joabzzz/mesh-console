//! HTTP client for the mesh-llm management API (default
//! `http://127.0.0.1:3131`) — the same surface the embedded React console
//! consumes (`/api/status`, `/api/models`, `/api/runtime*`, `/api/events`,
//! `/api/responses`, `/api/runtime/control-*`).
//!
//! [`ApiClient`] is base-URL-parametrized so tests can point it at a mock
//! server and the app can follow the embedded node onto alternate ports. The
//! module-level functions are thin wrappers over a client bound to
//! [`api_base()`], kept for the views' convenience.

pub mod control;
pub mod objects;
pub mod responses;
pub mod sse;

pub use control::ApplyOutcome;
pub use objects::AttachmentFile;
pub use responses::ChatStreamItem;

use crate::state::{
    ControlBootstrap, DiscoveredMesh, LlamaRuntimePayload, MeshModel, MeshStatus, RuntimeModel,
    RuntimeProcess,
};
use futures::StreamExt;
use serde::Deserialize;
use std::sync::RwLock;

pub const DEFAULT_API_BASE: &str = "http://127.0.0.1:3131";

/// Overridden when the embedded node starts on non-default ports.
static API_BASE_OVERRIDE: RwLock<Option<String>> = RwLock::new(None);

/// Point the management-API client at a different base URL (e.g. the embedded
/// node's `console_url()` when it had to start on alternate ports).
pub fn set_api_base(url: impl Into<String>) {
    let url = url.into();
    let mut guard = API_BASE_OVERRIDE.write().expect("api base lock");
    *guard = Some(url.trim_end_matches('/').to_string());
}

/// Current management-API base URL.
pub fn api_base() -> String {
    API_BASE_OVERRIDE
        .read()
        .expect("api base lock")
        .clone()
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    mesh_models: Vec<MeshModel>,
}

#[derive(Debug, Deserialize)]
struct RuntimeStatusResponse {
    #[serde(default)]
    models: Vec<RuntimeModel>,
}

#[derive(Debug, Deserialize)]
struct RuntimeProcessesResponse {
    #[serde(default)]
    processes: Vec<RuntimeProcess>,
}

/// Management-API client bound to one base URL.
#[derive(Clone)]
pub struct ApiClient {
    base: String,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Client bound to the current global base URL.
    pub fn current() -> Self {
        Self::new(api_base())
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<T, String> {
        let resp = self
            .http
            .get(self.url(path))
            .send()
            .await
            .map_err(|e| format!("{what} request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("{what} HTTP {}", resp.status()));
        }
        resp.json::<T>()
            .await
            .map_err(|e| format!("{what} parse error: {e}"))
    }

    pub async fn fetch_status(&self) -> Result<MeshStatus, String> {
        self.get_json("/api/status", "status").await
    }

    pub async fn fetch_models(&self) -> Result<Vec<MeshModel>, String> {
        let parsed: ModelsResponse = self.get_json("/api/models", "models").await?;
        Ok(parsed.mesh_models)
    }

    pub async fn fetch_runtime_status(&self) -> Result<Vec<RuntimeModel>, String> {
        let parsed: RuntimeStatusResponse = self.get_json("/api/runtime", "runtime").await?;
        Ok(parsed.models)
    }

    pub async fn fetch_runtime_processes(&self) -> Result<Vec<RuntimeProcess>, String> {
        let parsed: RuntimeProcessesResponse =
            self.get_json("/api/runtime/processes", "processes").await?;
        Ok(parsed.processes)
    }

    pub async fn fetch_runtime_llama(&self) -> Result<LlamaRuntimePayload, String> {
        self.get_json("/api/runtime/llama", "llama runtime").await
    }

    pub async fn fetch_discover(&self) -> Result<Vec<DiscoveredMesh>, String> {
        self.get_json("/api/discover", "discovery").await
    }

    pub async fn fetch_control_bootstrap(&self) -> Result<ControlBootstrap, String> {
        self.get_json("/api/runtime/control-bootstrap", "control bootstrap")
            .await
    }

    pub async fn load_model(&self, model_name: &str) -> Result<(), String> {
        let body = serde_json::json!({ "model": model_name });
        let resp = self
            .http
            .post(self.url("/api/runtime/models"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(format!("HTTP {status}: {text}"))
        }
    }

    pub async fn unload_model(&self, model_name: &str) -> Result<(), String> {
        let resp = self
            .http
            .delete(self.url(&format!("/api/runtime/models/{model_name}")))
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(format!("HTTP {status}: {text}"))
        }
    }

    /// Connect to `/api/events` and forward parsed `MeshStatus` snapshots
    /// through `tx` until either side disconnects.
    ///
    /// The endpoint pushes one `data: <status-json>` SSE frame on connect and
    /// one per server-side update. Returns `Ok(())` on a clean close and an
    /// `Err` describing why the connection or a frame failed; callers
    /// typically loop with backoff to reconnect.
    pub async fn stream_status(
        &self,
        tx: tokio::sync::mpsc::UnboundedSender<MeshStatus>,
    ) -> Result<(), String> {
        let resp = self
            .http
            .get(self.url("/api/events"))
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| format!("events connect failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("events HTTP {}", resp.status()));
        }

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("events read failed: {e}"))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            for payload in sse::drain_data_payloads(&mut buf) {
                if let Ok(status) = serde_json::from_str::<MeshStatus>(&payload) {
                    if tx.send(status).is_err() {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}

// ── Module-level wrappers bound to the global base URL ───────────────────────

pub async fn fetch_status() -> Result<MeshStatus, String> {
    ApiClient::current().fetch_status().await
}

pub async fn fetch_models() -> Result<Vec<MeshModel>, String> {
    ApiClient::current().fetch_models().await
}

#[allow(dead_code)]
pub async fn fetch_runtime_status() -> Result<Vec<RuntimeModel>, String> {
    ApiClient::current().fetch_runtime_status().await
}

pub async fn fetch_runtime_processes() -> Result<Vec<RuntimeProcess>, String> {
    ApiClient::current().fetch_runtime_processes().await
}

pub async fn fetch_runtime_llama() -> Result<LlamaRuntimePayload, String> {
    ApiClient::current().fetch_runtime_llama().await
}

pub async fn fetch_discover() -> Result<Vec<DiscoveredMesh>, String> {
    ApiClient::current().fetch_discover().await
}

pub async fn load_model(model_name: &str) -> Result<(), String> {
    ApiClient::current().load_model(model_name).await
}

pub async fn unload_model(model_name: &str) -> Result<(), String> {
    ApiClient::current().unload_model(model_name).await
}

pub async fn stream_status(
    tx: tokio::sync::mpsc::UnboundedSender<MeshStatus>,
) -> Result<(), String> {
    ApiClient::current().stream_status(tx).await
}

/// Build a stable key from the parts of `MeshStatus` that change when the
/// mesh model catalog changes. Mirrors `modelCatalogKeyFromStatus` in the
/// embedded React UI so we can avoid re-fetching `/api/models` on every
/// status tick.
pub fn model_catalog_key(status: &MeshStatus) -> String {
    let mut local = vec![status.model_name.clone()];
    local.extend(status.models.iter().cloned());
    local.extend(status.available_models.iter().cloned());
    local.extend(status.requested_models.iter().cloned());
    local.extend(status.serving_models.iter().cloned());
    local.extend(status.hosted_models.iter().cloned());
    let local = local.join(",");

    let mut peer_keys: Vec<String> = status
        .peers
        .iter()
        .map(|peer| {
            let mut parts = vec![peer.id.clone()];
            parts.extend(peer.models.iter().cloned());
            parts.extend(peer.available_models.iter().cloned());
            parts.extend(peer.serving_models.iter().cloned());
            parts.extend(peer.hosted_models.iter().cloned());
            parts.join(",")
        })
        .collect();
    peer_keys.sort();
    let peers = peer_keys.join("|");

    format!("{}::{local}::{peers}", status.node_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PeerInfo;

    fn status_with(node_id: &str, models: &[&str]) -> MeshStatus {
        MeshStatus {
            node_id: node_id.to_string(),
            models: models.iter().map(|s| s.to_string()).collect(),
            ..MeshStatus::default()
        }
    }

    #[test]
    fn catalog_key_changes_with_models_not_noise() {
        let a = status_with("n1", &["qwen"]);
        let mut b = status_with("n1", &["qwen"]);
        b.inflight_requests = 42; // noise field, not part of the key
        assert_eq!(model_catalog_key(&a), model_catalog_key(&b));

        let c = status_with("n1", &["qwen", "llama"]);
        assert_ne!(model_catalog_key(&a), model_catalog_key(&c));
    }

    #[test]
    fn catalog_key_is_order_independent_across_peers() {
        let peer = |id: &str| PeerInfo {
            id: id.to_string(),
            ..PeerInfo::default()
        };
        let mut a = status_with("n1", &[]);
        a.peers = vec![peer("p1"), peer("p2")];
        let mut b = status_with("n1", &[]);
        b.peers = vec![peer("p2"), peer("p1")];
        assert_eq!(model_catalog_key(&a), model_catalog_key(&b));
    }
}
