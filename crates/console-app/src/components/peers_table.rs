//! Detailed peers table for the Network view — the web console's PeersTable
//! equivalent: state, latency (+ source), VRAM, version, owner/attestation
//! badges, GPUs, and served models per peer.

use crate::state::{MeshStatus, PeerInfo};
use crate::status_display;
use dioxus::prelude::*;

#[component]
pub fn PeersTable(status: Signal<MeshStatus>) -> Element {
    let st = status.read();
    let peers = st.peers.clone();
    let self_hostname = st.my_hostname.clone().unwrap_or_else(|| "—".to_string());
    let self_state = if st.node_state.is_empty() {
        status_display::local_status_label(&st).to_lowercase()
    } else {
        st.node_state.clone()
    };
    let self_vram = st.my_vram_gb;
    let self_version = st.version.clone();
    let self_models = st.serving_models.clone();
    let self_owner_verified = st.owner.as_ref().map(|o| o.verified).unwrap_or(false);
    drop(st);

    rsx! {
        section { class: "section",
            div { class: "section-header-row",
                h2 { class: "section-title", "Peers" }
                span { class: "row-meta", "{peers.len() + 1} nodes" }
            }
            div { class: "peers-table",
                div { class: "peers-table-head",
                    span { "node" }
                    span { "state" }
                    span { "latency" }
                    span { "vram" }
                    span { "version" }
                    span { "models" }
                }
                // Self row first, like the web console.
                div { class: "peers-table-row peers-table-row--self",
                    span { class: "peers-cell-name",
                        "{self_hostname}"
                        span { class: "badge badge--info", "this device" }
                        if self_owner_verified {
                            span { class: "badge badge--green", title: "Owner verified", "✓" }
                        }
                    }
                    span { StateBadge { state: self_state.clone() } }
                    span { class: "row-meta", "—" }
                    span { class: "row-meta", "{self_vram:.0} GB" }
                    span { class: "row-meta", "v{self_version}" }
                    span { class: "peers-cell-models",
                        for m in self_models.iter().filter(|m| !m.is_empty()) {
                            span { class: "chip chip--good", "{m}" }
                        }
                    }
                }
                for peer in peers.iter() {
                    PeerRow { peer: peer.clone() }
                }
            }
        }
    }
}

#[component]
fn PeerRow(peer: PeerInfo) -> Element {
    let name = status_display::peer_display_name(&peer);
    let state = if peer.state.is_empty() {
        status_display::peer_status_label(&peer).to_lowercase()
    } else {
        peer.state.clone()
    };
    let latency = peer
        .latency_ms
        .or(peer.rtt_ms)
        .map(|ms| format!("{ms} ms"))
        .unwrap_or_else(|| "—".to_string());
    let latency_source = peer.latency_source.clone().unwrap_or_default();
    let version = peer
        .version
        .clone()
        .map(|v| format!("v{v}"))
        .unwrap_or_else(|| "—".to_string());
    let owner_verified = peer.owner.as_ref().map(|o| o.verified).unwrap_or(false);
    let attestation_ok = peer
        .release_attestation
        .as_ref()
        .map(|a| a.verified)
        .unwrap_or(false);
    let models = status_display::peer_routable_models(&peer);
    let gpus: Vec<String> = peer.gpus.iter().map(|g| g.name.clone()).collect();

    rsx! {
        div { class: "peers-table-row",
            span { class: "peers-cell-name",
                "{name}"
                if owner_verified {
                    span { class: "badge badge--green", title: "Owner verified", "✓" }
                }
                if attestation_ok {
                    span { class: "badge badge--dim", title: "Release attestation valid", "signed" }
                }
                if !gpus.is_empty() {
                    span { class: "row-meta peers-cell-gpu", "{gpus.join(\", \")}" }
                }
            }
            span { StateBadge { state: state.clone() } }
            span { class: "row-meta",
                "{latency}"
                if latency_source == "estimated" {
                    span { class: "row-meta peers-latency-source", " (est.)" }
                }
            }
            span { class: "row-meta", "{peer.vram_gb:.0} GB" }
            span { class: "row-meta", "{version}" }
            span { class: "peers-cell-models",
                for m in models.iter().filter(|m| *m != "(idle)") {
                    span { class: "chip chip--good", "{m}" }
                }
            }
        }
    }
}

#[component]
fn StateBadge(state: String) -> Element {
    let class = match state.as_str() {
        "serving" => "badge badge--green",
        "loading" => "badge badge--warn",
        "client" => "badge badge--info",
        _ => "badge badge--dim",
    };
    rsx! {
        span { class: "{class}", "{state}" }
    }
}
