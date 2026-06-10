//! `/api/models` catalog entries and `/api/runtime*` process listings.
//! Mirrors `MeshModelPayload` / `RuntimeModelPayload` in
//! `mesh-llm-host-runtime/src/api/status.rs`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshModel {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    /// "warm" | "cold"
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default)]
    pub mesh_vram_gb: f64,
    #[serde(default)]
    pub size_gb: f64,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub context_length: Option<u32>,
    #[serde(default)]
    pub quantization: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub multimodal: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_use: bool,
    #[serde(default)]
    pub moe: bool,
    #[serde(default)]
    pub draft_model: Option<String>,
    #[serde(default)]
    pub fit_label: String,
    #[serde(default)]
    pub fit_detail: String,
    #[serde(default)]
    pub request_count: Option<u64>,
    #[serde(default)]
    pub last_active_secs_ago: Option<u64>,
    #[serde(default)]
    pub active_nodes: Vec<String>,
    #[serde(default)]
    pub wanted: Option<bool>,
    #[serde(default)]
    pub target_rank: Option<usize>,
    #[serde(default)]
    pub source_page_url: Option<String>,
    #[serde(default)]
    pub download_command: String,
    #[serde(default)]
    pub run_command: String,
}

/// One runtime-managed model process (from the `runtime.models` status block
/// and `/api/runtime`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuntimeModel {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub port: Option<u16>,
}

/// `/api/runtime/processes` row (PID-level detail).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuntimeProcess {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub pid: u32,
}
