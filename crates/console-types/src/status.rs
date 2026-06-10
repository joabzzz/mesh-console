//! `/api/status` (and the `/api/events` SSE stream, which carries the same
//! snapshot). Mirror of `StatusPayload` in
//! `mesh-llm-host-runtime/src/api/status.rs`.

use crate::models::{MeshModel, RuntimeModel};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoutingAffinity {
    #[serde(default)]
    pub prefix_enabled: bool,
    #[serde(default)]
    pub sticky_enabled: bool,
    #[serde(default)]
    pub prefix_entries: u64,
    #[serde(default)]
    pub prefix_lookups: u64,
    #[serde(default)]
    pub prefix_hits: u64,
    #[serde(default)]
    pub prefix_misses: u64,
    #[serde(default)]
    pub prefix_stale: u64,
    #[serde(default)]
    pub prefix_routes: u64,
    #[serde(default)]
    pub sticky_routes: u64,
    #[serde(default)]
    pub session_routes: u64,
    #[serde(default)]
    pub learned: u64,
    #[serde(default)]
    pub evicted: u64,
}

/// Owner identity attached to this node or a peer ("verified", "unsigned",
/// "expired", …).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OwnershipInfo {
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub cert_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub expires_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub node_label: Option<String>,
    #[serde(default)]
    pub hostname_hint: Option<String>,
}

/// Release-signing verification of the node binary ("valid" | "missing" |
/// "invalid").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReleaseAttestationSummary {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub signer_key_id: Option<String>,
    #[serde(default)]
    pub node_version: Option<String>,
    #[serde(default)]
    pub build_id: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub target_triple: Option<String>,
    #[serde(default)]
    pub artifact_digest: Option<String>,
    #[serde(default)]
    pub issued_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub expires_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub supported_protocol_generation_min: Option<u32>,
    #[serde(default)]
    pub supported_protocol_generation_max: Option<u32>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub verified: bool,
}

/// Local runtime status block embedded in `/api/status` (backend + per-model
/// process state + pipeline stages).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub models: Vec<RuntimeModel>,
    #[serde(default)]
    pub stages: Vec<RuntimeStageInfo>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStageInfo {
    #[serde(default)]
    pub stage_id: String,
    #[serde(default)]
    pub topology_id: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub layer_start: u32,
    #[serde(default)]
    pub layer_end: u32,
    #[serde(default)]
    pub state: String,
}

/// A powered-down node that the mesh can wake on demand (Reserves page).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WakeableNode {
    #[serde(default)]
    pub logical_id: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub vram_gb: f64,
    #[serde(default)]
    pub provider: Option<String>,
    /// "sleeping" | "waking"
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub wake_eta_secs: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeshStatus {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub owner: Option<OwnershipInfo>,
    #[serde(default)]
    pub release_attestation: Option<ReleaseAttestationSummary>,
    #[serde(default)]
    pub token: String,
    /// "client" | "standby" | "loading" | "serving"
    #[serde(default)]
    pub node_state: String,
    #[serde(default)]
    pub node_status: String,
    #[serde(default)]
    pub is_host: bool,
    #[serde(default)]
    pub is_client: bool,
    #[serde(default)]
    pub llama_ready: bool,
    #[serde(default)]
    pub runtime: Option<RuntimeInfo>,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub available_models: Vec<String>,
    #[serde(default)]
    pub requested_models: Vec<String>,
    #[serde(default)]
    pub wanted_model_refs: Vec<String>,
    #[serde(default)]
    pub serving_models: Vec<String>,
    #[serde(default)]
    pub hosted_models: Vec<String>,
    #[serde(default)]
    pub draft_name: Option<String>,
    #[serde(default)]
    pub api_port: u16,
    #[serde(default)]
    pub my_vram_gb: f64,
    #[serde(default)]
    pub model_size_gb: f64,
    #[serde(default)]
    pub peers: Vec<PeerInfo>,
    #[serde(default)]
    pub wakeable_nodes: Vec<WakeableNode>,
    #[serde(default)]
    pub local_instances: Vec<LocalInstance>,
    #[serde(default)]
    pub launch_pi: Option<String>,
    #[serde(default)]
    pub launch_goose: Option<String>,
    #[serde(default)]
    pub inflight_requests: u64,
    #[serde(default)]
    pub mesh_id: Option<String>,
    #[serde(default)]
    pub mesh_name: Option<String>,
    #[serde(default)]
    pub mesh_discovery_mode: String,
    #[serde(default)]
    pub nostr_discovery: bool,
    /// "private" | "public" | "publish_failed"
    #[serde(default)]
    pub publication_state: String,
    #[serde(default)]
    pub my_hostname: Option<String>,
    #[serde(default)]
    pub my_is_soc: Option<bool>,
    #[serde(default)]
    pub gpus: Vec<GpuEntry>,
    #[serde(default)]
    pub routing_affinity: Option<RoutingAffinity>,
    #[serde(default)]
    pub first_joined_mesh_ts: Option<u64>,
    /// Mesh model catalog. NOT part of the `/api/status` wire payload — the
    /// app merges `/api/models` into the status snapshot here (mirrors the
    /// React UI's catalog handling).
    #[serde(default)]
    pub mesh_models: Vec<MeshModel>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub owner: Option<OwnershipInfo>,
    #[serde(default)]
    pub release_attestation: Option<ReleaseAttestationSummary>,
    #[serde(default)]
    pub role: String,
    /// "client" | "standby" | "loading" | "serving"
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub available_models: Vec<String>,
    #[serde(default)]
    pub requested_models: Vec<String>,
    #[serde(default)]
    pub serving_models: Vec<String>,
    #[serde(default)]
    pub hosted_models: Vec<String>,
    /// Mirrors mesh-llm `PeerPayload.hosted_models_known`. When absent (older
    /// daemons), assume hosts publish hosted models so routable model
    /// inference falls back to `serving_models` only when explicitly unknown.
    #[serde(default = "default_true")]
    pub hosted_models_known: bool,
    #[serde(default)]
    pub vram_gb: f64,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub rtt_ms: Option<u32>,
    #[serde(default)]
    pub latency_ms: Option<u32>,
    /// "direct" | "estimated" | "unknown"
    #[serde(default)]
    pub latency_source: Option<String>,
    #[serde(default)]
    pub latency_age_ms: Option<u64>,
    #[serde(default)]
    pub latency_observer_id: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub is_soc: Option<bool>,
    #[serde(default)]
    pub gpus: Vec<GpuEntry>,
    #[serde(default)]
    pub first_joined_mesh_ts: Option<u64>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GpuEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vram_bytes: u64,
    #[serde(default)]
    pub reserved_bytes: Option<u64>,
    #[serde(default, alias = "bandwidth_gbps")]
    pub mem_bandwidth_gbps: Option<f64>,
    #[serde(default)]
    pub compute_tflops_fp32: Option<f64>,
    #[serde(default)]
    pub compute_tflops_fp16: Option<f64>,
    // Not in today's wire format, but the web console types reserve them.
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub utilization: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalInstance {
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub api_port: Option<u16>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub started_at_unix: i64,
    #[serde(default)]
    pub runtime_dir: String,
    #[serde(default)]
    pub is_self: bool,
}

impl GpuEntry {
    pub fn vram_gb(&self) -> f64 {
        self.vram_bytes as f64 / 1_073_741_824.0
    }
}
