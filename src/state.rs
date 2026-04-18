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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeshStatus {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub node_status: String,
    #[serde(default)]
    pub is_host: bool,
    #[serde(default)]
    pub is_client: bool,
    #[serde(default)]
    pub llama_ready: bool,
    #[serde(default)]
    pub model_name: String,
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
    #[serde(default)]
    pub my_vram_gb: f64,
    #[serde(default)]
    pub model_size_gb: f64,
    #[serde(default)]
    pub peers: Vec<PeerInfo>,
    #[serde(default)]
    pub mesh_models: Vec<MeshModel>,
    #[serde(default)]
    pub inflight_requests: u64,
    #[serde(default)]
    pub mesh_id: Option<String>,
    #[serde(default)]
    pub mesh_name: Option<String>,
    #[serde(default)]
    pub my_hostname: Option<String>,
    #[serde(default)]
    pub nostr_discovery: bool,
    #[serde(default)]
    pub gpus: Vec<GpuEntry>,
    #[serde(default)]
    pub api_port: u16,
    #[serde(default)]
    pub draft_name: Option<String>,
    #[serde(default)]
    pub my_is_soc: Option<bool>,
    #[serde(default)]
    pub launch_pi: Option<String>,
    #[serde(default)]
    pub launch_goose: Option<String>,
    #[serde(default)]
    pub routing_affinity: Option<RoutingAffinity>,
    #[serde(default)]
    pub local_instances: Vec<LocalInstance>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub role: String,
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
    pub rtt_ms: Option<u32>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub is_soc: Option<bool>,
    #[serde(default)]
    pub gpus: Vec<GpuEntry>,
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshModel {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
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
    pub fit_label: String,
    #[serde(default)]
    pub fit_detail: String,
    #[serde(default)]
    pub request_count: Option<u64>,
    #[serde(default)]
    pub last_active_secs_ago: Option<u64>,
    #[serde(default)]
    pub active_nodes: Vec<String>,
}

#[allow(dead_code)]
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

// ---------------------------------------------------------------------------
// Nostr discovery types (mirrors DiscoveredMesh from mesh-llm)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredMesh {
    pub listing: MeshListing,
    #[serde(default)]
    pub publisher_npub: String,
    #[serde(default)]
    pub published_at: u64,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshListing {
    #[serde(default)]
    pub invite_token: String,
    #[serde(default)]
    pub serving: Vec<String>,
    #[serde(default)]
    pub wanted: Vec<String>,
    #[serde(default)]
    pub on_disk: Vec<String>,
    #[serde(default)]
    pub total_vram_bytes: u64,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default)]
    pub client_count: usize,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub mesh_id: Option<String>,
}

impl GpuEntry {
    #[allow(dead_code)]
    pub fn vram_gb(&self) -> f64 {
        self.vram_bytes as f64 / 1_073_741_824.0
    }
}

impl MeshListing {
    #[allow(dead_code)]
    pub fn total_vram_gb(&self) -> f64 {
        self.total_vram_bytes as f64 / 1_073_741_824.0
    }
}
