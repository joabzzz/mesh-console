//! Reserves — wakeable nodes the mesh can power on for extra capacity.
//! Rendered as a section inside Settings (provider groups, sleeping/waking
//! state, wake ETA, and VRAM capacity summaries, all from
//! `status.wakeable_nodes`), mirroring the real-data surface of the web
//! console's Reserves page.

use crate::state::MeshStatus;
use console_core::vm::reserves::{group_by_provider, totals, wake_eta_label};
use dioxus::prelude::*;

#[component]
pub fn ReservesSection(status: Signal<MeshStatus>, connected: Signal<bool>) -> Element {
    let st = status.read();
    let nodes = st.wakeable_nodes.clone();
    let live_vram: f64 = st.my_vram_gb
        + st.peers
            .iter()
            .filter(|p| p.role != "Client")
            .map(|p| p.vram_gb)
            .sum::<f64>();
    drop(st);

    let conn = *connected.read();
    let summary = totals(&nodes);
    let groups = group_by_provider(&nodes);

    rsx! {
        section { class: "section",
            div { class: "section-header-row",
                h2 { class: "section-title", "Reserves" }
                span { class: "section-count", "{summary.node_count} wakeable" }
            }

            if !conn {
                div { class: "empty-state",
                    span { "Daemon offline" }
                    span { class: "empty-hint", "Start or join a mesh to see wakeable capacity." }
                }
            } else if nodes.is_empty() {
                div { class: "empty-state",
                    span { "No wakeable nodes" }
                    span { class: "empty-hint",
                        "Nodes registered as wakeable (wake-on-LAN, cloud providers) appear here with their models and VRAM, ready to extend the mesh on demand."
                    }
                }
            } else {
                div { class: "stat-grid",
                    StatTile { label: "online VRAM", value: format!("{live_vram:.0} GB") }
                    StatTile { label: "reserve VRAM", value: format!("{:.0} GB", summary.total_vram_gb) }
                    StatTile { label: "sleeping", value: summary.sleeping.to_string() }
                    StatTile { label: "waking", value: summary.waking.to_string() }
                }

                for group in groups.iter() {
                    div { class: "reserve-group",
                        div { class: "section-header-row",
                            span { class: "row-name", "{group.provider}" }
                            span { class: "row-meta",
                                "{group.totals.node_count} nodes · {group.totals.total_vram_gb:.0} GB"
                            }
                        }
                        div { class: "row-list",
                            for node in group.nodes.iter() {
                                div { class: "row-item row-item--mesh",
                                    div { class: "row-main",
                                        div { class: "peer-header",
                                            span { class: "row-name", "{node.logical_id}" }
                                            if node.state == "waking" {
                                                span { class: "badge badge--green", "waking" }
                                            } else {
                                                span { class: "badge badge--dim", "sleeping" }
                                            }
                                            if let Some(eta) = wake_eta_label(node.wake_eta_secs) {
                                                span { class: "row-meta", "online {eta}" }
                                            }
                                        }
                                        div { class: "peer-meta",
                                            span { class: "row-meta", "{node.vram_gb:.0} GB VRAM" }
                                        }
                                        if !node.models.is_empty() {
                                            div { class: "cap-row",
                                                for m in node.models.iter() {
                                                    span { class: "badge badge--dim", "{m}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatTile(label: String, value: String) -> Element {
    rsx! {
        div { class: "stat-cell",
            span { class: "stat-label", "{label}" }
            span { class: "stat-value", "{value}" }
        }
    }
}
