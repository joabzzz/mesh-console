//! Display helpers for nodes and peers. Mirrors the small decision tree in
//! [`mesh-llm/ui/src/features/app-shell/lib/status-helpers.ts`] so the desktop
//! console renders the same labels as the embedded React dashboard.

use crate::state::{MeshStatus, PeerInfo};

/// Strip `-Q4_K_M` style quant suffixes and `-Instruct` for compact display.
/// Mirrors the JS regex `/-Q\w+$/` then `/-Instruct/`.
pub fn short_name(name: &str) -> String {
    let no_quant = match name.rfind("-Q") {
        Some(idx) => {
            let tail = &name[idx + 2..];
            let is_word =
                !tail.is_empty() && tail.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if is_word {
                &name[..idx]
            } else {
                name
            }
        }
        None => name,
    };
    no_quant.replace("-Instruct", "")
}

fn non_empty(values: &[String]) -> impl Iterator<Item = &String> {
    values.iter().filter(|m| !m.is_empty())
}

pub fn peer_assigned_models(peer: &PeerInfo) -> Vec<String> {
    non_empty(&peer.serving_models).cloned().collect()
}

/// Models the peer is willing to route requests for.
pub fn peer_routable_models(peer: &PeerInfo) -> Vec<String> {
    let hosted: Vec<String> = non_empty(&peer.hosted_models).cloned().collect();
    if !peer.hosted_models_known {
        if hosted.is_empty() {
            return peer_assigned_models(peer);
        }
        return hosted;
    }
    hosted
}

pub fn peer_primary_model(peer: &PeerInfo) -> Option<String> {
    peer_routable_models(peer)
        .into_iter()
        .next()
        .or_else(|| peer_assigned_models(peer).into_iter().next())
}

/// VRAM contributed to the mesh by a peer. Clients are routers; they don't
/// host parameters, so they count as zero against mesh VRAM totals.
pub fn overview_vram_gb(is_client: bool, vram_gb: f64) -> f64 {
    if is_client {
        0.0
    } else {
        vram_gb.max(0.0)
    }
}

/// Status label for a peer card pill.
pub fn peer_status_label(peer: &PeerInfo) -> &'static str {
    if peer.role == "Client" {
        return "Client";
    }
    if peer_routable_models(peer).iter().any(|m| m != "(idle)") {
        return "Serving";
    }
    if peer_assigned_models(peer).iter().any(|m| m != "(idle)") {
        return "Assigned";
    }
    if peer.role == "Host" {
        return "Host";
    }
    "Idle"
}

/// Aggregate VRAM across self + peers (clients excluded).
pub fn mesh_total_vram_gb(status: &MeshStatus) -> f64 {
    let self_vram = overview_vram_gb(status.is_client, status.my_vram_gb);
    let peers_vram: f64 = status
        .peers
        .iter()
        .map(|p| overview_vram_gb(p.role == "Client", p.vram_gb))
        .sum();
    self_vram + peers_vram
}

/// Best-effort SoC label: when `is_soc` is true and we know the GPU, use that
/// (e.g. "M4 Pro"); otherwise fall back to the GPU family or "Discrete".
pub fn soc_label(is_soc: bool, gpus: &[crate::state::GpuEntry]) -> Option<String> {
    let primary = gpus.first()?;
    if is_soc {
        let trimmed = primary
            .name
            .trim_start_matches("Apple ")
            .trim_start_matches("Apple Silicon ");
        if trimmed.is_empty() {
            None
        } else {
            Some(format!("SoC {trimmed}"))
        }
    } else if !primary.name.is_empty() {
        Some("Discrete GPU".to_string())
    } else {
        None
    }
}

/// Local node's status label, mirroring the same buckets as `peerStatusLabel`
/// for self.
pub fn local_status_label(status: &MeshStatus) -> &'static str {
    if status.is_client {
        return "Client";
    }
    let serving_real = status
        .serving_models
        .iter()
        .any(|m| !m.is_empty() && m != "(idle)");
    let hosted_real = status
        .hosted_models
        .iter()
        .any(|m| !m.is_empty() && m != "(idle)");
    if serving_real || hosted_real {
        return "Serving";
    }
    if status.is_host {
        return "Host";
    }
    "Idle"
}

/// Local node's primary model, mirroring `peer_primary_model` for self.
pub fn local_primary_model(status: &MeshStatus) -> Option<String> {
    if let Some(m) = status.serving_models.iter().find(|m| !m.is_empty()) {
        return Some(m.clone());
    }
    if let Some(m) = status.hosted_models.iter().find(|m| !m.is_empty()) {
        return Some(m.clone());
    }
    if !status.model_name.is_empty() {
        return Some(status.model_name.clone());
    }
    None
}

/// Hostname (or short node id fallback) for a peer.
pub fn peer_display_name(peer: &PeerInfo) -> String {
    peer.hostname.clone().unwrap_or_else(|| {
        if peer.id.len() > 12 {
            format!("{}…", &peer.id[..12])
        } else {
            peer.id.clone()
        }
    })
}
