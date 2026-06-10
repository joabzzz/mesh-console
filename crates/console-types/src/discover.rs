//! `/api/discover` — public meshes published over Nostr.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredMesh {
    #[serde(default)]
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

impl MeshListing {
    pub fn total_vram_gb(&self) -> f64 {
        self.total_vram_bytes as f64 / 1_073_741_824.0
    }
}
