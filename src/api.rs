//! HTTP client for the mesh-llm management API on `http://127.0.0.1:3131`.
//!
//! This is the operator-facing dashboard surface (`/api/status`, `/api/models`,
//! `/api/runtime*`, `/api/events`). The mesh-llm SDK ([`mesh_api`]) does not
//! expose any of these today, so the dashboard, topology, and runtime control
//! views all keep talking to `:3131` directly.
//!
//! Inference (chat / responses) goes through the SDK in
//! [`crate::sdk_session`] instead — that path targets the OpenAI proxy port
//! (`MeshStatus.api_port`, default 9337) rather than the management proxy on
//! `:3131`, which sidesteps the `503 LLM not ready` gate in
//! `mesh-llm/src/api/routes/chat.rs`.

use crate::state::{
    DiscoveredMesh, LocalInstance, MeshModel, MeshStatus, RuntimeModel, RuntimeProcess,
};
use futures::StreamExt;
use serde::Deserialize;

pub const API_BASE: &str = "http://127.0.0.1:3131";

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    mesh_models: Vec<MeshModel>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RuntimeStatusResponse {
    #[serde(default)]
    models: Vec<RuntimeModel>,
}

#[derive(Debug, Deserialize)]
struct RuntimeProcessesResponse {
    #[serde(default)]
    processes: Vec<RuntimeProcess>,
}

pub async fn fetch_status() -> Result<MeshStatus, String> {
    let url = format!("{API_BASE}/api/status");
    reqwest::get(&url)
        .await
        .map_err(|e| format!("connection failed: {e}"))?
        .json::<MeshStatus>()
        .await
        .map_err(|e| format!("parse error: {e}"))
}

pub async fn fetch_models() -> Result<Vec<MeshModel>, String> {
    let url = format!("{API_BASE}/api/models");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("models request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("models HTTP {}", resp.status()));
    }
    let parsed = resp
        .json::<ModelsResponse>()
        .await
        .map_err(|e| format!("models parse error: {e}"))?;
    Ok(parsed.mesh_models)
}

#[allow(dead_code)]
pub async fn fetch_runtime_status() -> Result<Vec<RuntimeModel>, String> {
    let url = format!("{API_BASE}/api/runtime");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("runtime request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("runtime HTTP {}", resp.status()));
    }
    let parsed = resp
        .json::<RuntimeStatusResponse>()
        .await
        .map_err(|e| format!("runtime parse error: {e}"))?;
    Ok(parsed.models)
}

pub async fn fetch_runtime_processes() -> Result<Vec<RuntimeProcess>, String> {
    let url = format!("{API_BASE}/api/runtime/processes");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("processes request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("processes HTTP {}", resp.status()));
    }
    let parsed = resp
        .json::<RuntimeProcessesResponse>()
        .await
        .map_err(|e| format!("processes parse error: {e}"))?;
    Ok(parsed.processes)
}

#[allow(dead_code)]
pub async fn fetch_discover() -> Result<Vec<DiscoveredMesh>, String> {
    let url = format!("{API_BASE}/api/discover");
    reqwest::get(&url)
        .await
        .map_err(|e| format!("discovery failed: {e}"))?
        .json::<Vec<DiscoveredMesh>>()
        .await
        .map_err(|e| format!("parse error: {e}"))
}

pub async fn load_model(model_name: &str) -> Result<(), String> {
    let url = format!("{API_BASE}/api/runtime/models");
    let body = serde_json::json!({ "model": model_name });
    let resp = reqwest::Client::new()
        .post(&url)
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

pub async fn unload_model(model_name: &str) -> Result<(), String> {
    let url = format!("{API_BASE}/api/runtime/models/{model_name}");
    let resp = reqwest::Client::new()
        .delete(&url)
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

/// Connect to `/api/events` and forward parsed `MeshStatus` snapshots through
/// `tx` until either side disconnects.
///
/// The endpoint pushes one `data: <status-json>` SSE frame on connect and one
/// per server-side update. The function returns `Ok(())` on a clean close and
/// an `Err` describing why the connection or a frame failed; callers typically
/// loop with backoff to reconnect.
pub async fn stream_status(
    tx: tokio::sync::mpsc::UnboundedSender<MeshStatus>,
) -> Result<(), String> {
    let url = format!("{API_BASE}/api/events");
    let resp = reqwest::Client::new()
        .get(&url)
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

        // SSE frames are separated by a blank line (\n\n). Drain complete
        // frames; keep any partial trailing frame in `buf`.
        while let Some(boundary) = buf.find("\n\n") {
            let frame: String = buf.drain(..boundary + 2).collect();
            for line in frame.lines() {
                if let Some(payload) = line.strip_prefix("data: ") {
                    if let Ok(status) = serde_json::from_str::<MeshStatus>(payload) {
                        if tx.send(status).is_err() {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn local_instance_summary(instances: &[LocalInstance]) -> Option<&LocalInstance> {
    instances.iter().find(|i| i.is_self).or(instances.first())
}
