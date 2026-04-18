use crate::state::{MeshStatus, PeerInfo};
use dioxus::prelude::*;

#[component]
pub fn PeersView(status: Signal<MeshStatus>) -> Element {
    let st = status.read();
    let peers: Vec<PeerInfo> = st.peers.clone();
    drop(st);

    rsx! {
        div { class: "view fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "Peers" }
                span { class: "count", " {peers.len()}" }
            }

            if peers.is_empty() {
                div { class: "empty-state",
                    span { "No peers connected" }
                    span { class: "empty-hint", "Other mesh-llm nodes will appear here." }
                }
            } else {
                div { class: "row-list",
                    for peer in peers.iter() {
                        PeerRow { peer: peer.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn PeerRow(peer: PeerInfo) -> Element {
    let hostname = peer.hostname.clone().unwrap_or_else(|| {
        if peer.id.len() > 12 {
            peer.id[..12].to_string() + "…"
        } else {
            peer.id.clone()
        }
    });

    let vram = if peer.vram_gb > 0.0 {
        format!("{:.1} GB", peer.vram_gb)
    } else {
        "—".to_string()
    };

    let rtt = peer
        .rtt_ms
        .map(|r| format!("{r} ms"))
        .unwrap_or_else(|| "—".to_string());

    let gpu_names: Vec<String> = peer.gpus.iter().map(|g| g.name.clone()).collect();
    let serving = peer.serving_models.clone();

    rsx! {
        div { class: "row-item row-item--peer",
            div { class: "row-main",
                div { class: "peer-header",
                    span { class: "row-name", "{hostname}" }
                    span { class: "badge badge--dim", "{peer.role}" }
                }
                div { class: "peer-meta",
                    span { class: "row-meta", "VRAM {vram}" }
                    span { class: "row-meta", "RTT {rtt}" }
                }
                if !gpu_names.is_empty() {
                    div { class: "cap-row",
                        for g in gpu_names.iter() {
                            span { class: "badge badge--dim", "{g}" }
                        }
                    }
                }
                if !serving.is_empty() {
                    div { class: "cap-row",
                        for m in serving.iter() {
                            span { class: "badge badge--green", "{m}" }
                        }
                    }
                }
            }
        }
    }
}
