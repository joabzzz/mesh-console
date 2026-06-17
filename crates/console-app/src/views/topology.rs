//! Mesh topology view: a flat constellation of nodes around the host.
//!
//! Each node is a glowing dot whose diameter scales with VRAM and whose fill
//! lifts when it is hosting a warm model. Peers sit on a ring around the
//! centre host, joined by thin spokes — matching the original mesh web
//! console's star-field look. Hovering or picking a dot swaps the detail card
//! shown below the stage. Status is conveyed by fill and outline, never colour.

use crate::components::qr::QrInviteButton;
use crate::state::{MeshStatus, PeerInfo};
use crate::status_display::{
    local_primary_model, local_status_label, mesh_total_vram_gb, overview_vram_gb,
    peer_display_name, peer_primary_model, peer_status_label, short_name, soc_label,
};
use dioxus::prelude::*;
use std::f64::consts::{PI, TAU};

#[derive(Clone, PartialEq)]
struct SceneNode {
    id: String,
    label: String,
    primary_model: Option<String>,
    status_label: String,
    role: String,
    is_self: bool,
    is_client: bool,
    is_warm: bool,
    vram_gb: f64,
    hostname: Option<String>,
    is_soc: bool,
    soc_text: Option<String>,
    /// Ring position in CSS pixels, relative to the scene centre.
    x: f64,
    y: f64,
    angle_deg: f64,
}

/// VRAM-driven dot diameter in CSS pixels. Clients and zero-VRAM nodes still
/// render at a visible minimum so they can be picked; the local node gets a
/// floor bump so the host reads as the anchor of the constellation.
fn dot_size_for(vram_gb: f64, is_client: bool, is_self: bool) -> f64 {
    let min = if is_client {
        12.0
    } else if is_self {
        30.0
    } else {
        16.0
    };
    let scaled = if vram_gb > 0.0 {
        vram_gb.sqrt() * 4.5
    } else {
        0.0
    };
    (min + scaled).clamp(10.0, 46.0)
}

/// Short label shown beneath a dot. Prefers hostname, falls back to the
/// node's display label, and truncates so long ids don't crowd the field.
fn node_short_label(node: &SceneNode) -> String {
    let raw = node
        .hostname
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| node.label.clone());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "node".to_string()
    } else if trimmed.chars().count() > 14 {
        let truncated: String = trimmed.chars().take(12).collect();
        format!("{truncated}…")
    } else {
        trimmed.to_string()
    }
}

impl SceneNode {
    fn from_self(status: &MeshStatus) -> Self {
        let gpus = &status.gpus;
        let is_soc = status.my_is_soc.unwrap_or(false);
        let is_client = status.is_client;
        let vram_gb = status.my_vram_gb;
        Self {
            id: status.node_id.clone(),
            label: status.my_hostname.clone().unwrap_or_else(|| {
                if status.node_id.len() > 12 {
                    format!("{}…", &status.node_id[..12])
                } else {
                    status.node_id.clone()
                }
            }),
            primary_model: local_primary_model(status),
            status_label: local_status_label(status).to_string(),
            role: if status.is_host {
                "Host".to_string()
            } else if is_client {
                "Client".to_string()
            } else {
                "Worker".to_string()
            },
            is_self: true,
            is_client,
            is_warm: false,
            vram_gb,
            hostname: status.my_hostname.clone(),
            is_soc,
            soc_text: soc_label(is_soc, gpus),
            x: 0.0,
            y: 0.0,
            angle_deg: 0.0,
        }
    }

    fn from_peer(peer: &PeerInfo, angle_rad: f64, radius: f64) -> Self {
        let is_soc = peer.is_soc.unwrap_or(false);
        let is_client = peer.role == "Client";
        let vram_gb = peer.vram_gb;
        Self {
            id: peer.id.clone(),
            label: peer_display_name(peer),
            primary_model: peer_primary_model(peer),
            status_label: peer_status_label(peer).to_string(),
            role: peer.role.clone(),
            is_self: false,
            is_client,
            is_warm: false,
            vram_gb,
            hostname: peer.hostname.clone(),
            is_soc,
            soc_text: soc_label(is_soc, &peer.gpus),
            x: radius * angle_rad.cos(),
            y: radius * angle_rad.sin(),
            angle_deg: angle_rad.to_degrees(),
        }
    }
}

fn build_scene(status: &MeshStatus, radius: f64) -> Vec<SceneNode> {
    let mut nodes = Vec::with_capacity(1 + status.peers.len());
    if !status.node_id.is_empty() {
        nodes.push(SceneNode::from_self(status));
    }
    let n = status.peers.len() as f64;
    // Start at the top of the ring (12 o'clock) and walk clockwise.
    let start = -TAU / 4.0;
    for (i, peer) in status.peers.iter().enumerate() {
        let angle = start + (i as f64 / n.max(1.0)) * TAU;
        nodes.push(SceneNode::from_peer(peer, angle, radius));
    }
    for node in nodes.iter_mut() {
        node.is_warm = node_is_warm(node);
    }
    nodes
}

/// Ring radius for the peer dots: nudge it up for modest peer counts so dots
/// get a little more chord spacing, hard-capped so the constellation stays
/// inside the fixed-height `.topology-stage`.
fn orbit_radius(peer_count: usize) -> f64 {
    const DOT_CLEAR: f64 = 64.0;
    const PAD: f64 = 18.0;
    const R_MIN: f64 = 150.0;
    const R_CAP: f64 = 210.0;
    let need = DOT_CLEAR + PAD;
    match peer_count {
        0 => R_MIN,
        1 => (180.0_f64).clamp(R_MIN, R_CAP),
        n => {
            let n = n as f64;
            let r = need / (2.0 * (PI / n).sin().max(1e-6));
            r.clamp(R_MIN, R_CAP)
        }
    }
}

/// A node is "warm" — visually luminous — when it is actively running
/// inference. Derived from the resolved status label so the dot glow stays in
/// sync with the per-node status pill: `Serving`, `Serving (split)`, or
/// `Assigned` (worker holding warm shards) light up.
fn node_is_warm(node: &SceneNode) -> bool {
    matches!(
        node.status_label.as_str(),
        "Serving" | "Serving (split)" | "Assigned"
    )
}

fn status_tone(label: &str) -> &'static str {
    match label {
        "Serving" => "tone-good",
        "Assigned" => "tone-warn",
        "Client" => "tone-info",
        "Host" => "tone-info",
        _ => "tone-dim",
    }
}

#[component]
pub fn TopologyView(status: Signal<MeshStatus>) -> Element {
    let st = status.read();
    let peer_count = st.peers.len();
    let radius = orbit_radius(peer_count);
    let scene = build_scene(&st, radius);
    let total_vram = mesh_total_vram_gb(&st);
    let token = st.token.clone();
    let self_id = st.node_id.clone();
    drop(st);

    let mut selected_id = use_signal(String::new);
    let mut hover_id = use_signal(String::new);
    let current_selection = selected_id.read().clone();
    let current_hover = hover_id.read().clone();
    // Visible selection ring + bottom detail card both follow `displayed_id`:
    // hover wins so moving the mouse around previews each dot; click pins a
    // selection that persists once the mouse leaves the field.
    let displayed_id = if !current_hover.is_empty() {
        current_hover
    } else if !current_selection.is_empty() {
        current_selection
    } else {
        self_id.clone()
    };
    let displayed_node = scene.iter().find(|n| n.id == displayed_id).cloned();

    let ring_outer = format!("width: {d:.0}px; height: {d:.0}px;", d = radius * 2.0);
    let ring_inner = format!("width: {d:.0}px; height: {d:.0}px;", d = radius);

    rsx! {
        section { class: "section",
            div { class: "section-header-row",
                div { class: "section-title-row",
                    h2 { class: "section-title", "Network" }
                    span { class: "section-count", "{1 + peer_count} nodes" }
                }
                div { class: "topology-controls",
                    if !token.is_empty() {
                        QrInviteButton { token: token.clone(), label: "QR invite".to_string() }
                    }
                }
            }

            div { class: "topology-stage",
                div { class: "topology-scene constellation",
                    div { class: "constellation-ring", style: "{ring_outer}" }
                    div { class: "constellation-ring", style: "{ring_inner}" }
                    for node in scene.iter().filter(|n| !n.is_self) {
                        SpokeLine {
                            angle_deg: node.angle_deg,
                            length: radius,
                            tone: status_tone(&node.status_label),
                        }
                    }
                    for node in scene.iter() {
                        ConstellationNode {
                            key: "{node.id}",
                            node: node.clone(),
                            selected: displayed_id == node.id,
                            on_select: {
                                let id = node.id.clone();
                                move |_| selected_id.set(id.clone())
                            },
                            on_hover_enter: {
                                let id = node.id.clone();
                                move |_| hover_id.set(id.clone())
                            },
                            on_hover_leave: move |_| hover_id.set(String::new()),
                        }
                    }
                }
            }

            if let Some(node) = displayed_node {
                div { class: "topology-detail-row",
                    NodeCardDetail {
                        node,
                        mesh_total_vram_gb: total_vram,
                    }
                }
            }
        }
    }
}

/// One node in the constellation: a glowing dot sized by VRAM with a label
/// beneath it. Acts as the click target and the hover preview source for the
/// detail card below the stage.
#[component]
fn ConstellationNode(
    node: SceneNode,
    selected: bool,
    on_select: EventHandler<()>,
    on_hover_enter: EventHandler<()>,
    on_hover_leave: EventHandler<()>,
) -> Element {
    let tone = status_tone(&node.status_label);
    let size = dot_size_for(node.vram_gb, node.is_client, node.is_self);
    let mut classes = vec![
        "constellation-node".to_string(),
        format!("constellation-node--{tone}"),
    ];
    if node.is_self {
        classes.push("constellation-node--self".to_string());
    }
    if node.is_warm {
        classes.push("constellation-node--warm".to_string());
    }
    if node.is_client {
        classes.push("constellation-node--client".to_string());
    }
    if selected {
        classes.push("constellation-node--selected".to_string());
    }
    let class_attr = classes.join(" ");

    let style = format!(
        "width: {size:.1}px; height: {size:.1}px; left: 50%; top: 50%; \
         transform: translate(calc(-50% + {x:.2}px), calc(-50% + {y:.2}px));",
        x = node.x,
        y = node.y,
    );

    let title_text = node
        .primary_model
        .clone()
        .map(|m| short_name(&m))
        .unwrap_or_else(|| node.label.clone());
    let tooltip = if node.vram_gb > 0.0 && !node.is_client {
        format!(
            "{} • {} • {:.1} GB VRAM • {}",
            node.label, title_text, node.vram_gb, node.status_label
        )
    } else {
        format!("{} • {} • {}", node.label, title_text, node.status_label)
    };

    let label_text = node_short_label(&node);

    rsx! {
        button {
            class: "{class_attr}",
            style: "{style}",
            title: "{tooltip}",
            "aria-label": "{tooltip}",
            onclick: move |_| on_select.call(()),
            onmouseenter: move |_| on_hover_enter.call(()),
            onmouseleave: move |_| on_hover_leave.call(()),
            onfocus: move |_| on_hover_enter.call(()),
            onblur: move |_| on_hover_leave.call(()),
            span { class: "constellation-dot" }
            span { class: "constellation-node-label", "{label_text}" }
        }
    }
}

#[component]
fn SpokeLine(angle_deg: f64, length: f64, tone: String) -> Element {
    // A thin bar pointing along +X, then rotated to point at the peer angle.
    // Positioned at the centre of the scene.
    let style = format!("transform: rotate({angle_deg:.2}deg); width: {length:.2}px;");
    rsx! {
        div { class: "topology-spoke topology-spoke--{tone}", style: "{style}" }
    }
}

/// Detail card for the hovered/selected node, rendered as a plain card under
/// the constellation stage so the picked node shows full info.
#[component]
fn NodeCardDetail(node: SceneNode, mesh_total_vram_gb: f64) -> Element {
    let tone = status_tone(&node.status_label);
    let tone_modifier = format!("node-card--{tone}");
    let mut classes = vec!["node-card", "node-card-detail"];
    classes.push(&tone_modifier);
    if node.is_self {
        classes.push("node-card-detail--self");
    }
    let class_attr = classes.join(" ");

    rsx! {
        div { class: "{class_attr}",
            NodeCardBody { node: node.clone(), mesh_total_vram_gb }
        }
    }
}

/// Shared head + pill row for the detail card.
#[component]
fn NodeCardBody(node: SceneNode, mesh_total_vram_gb: f64) -> Element {
    let tone = status_tone(&node.status_label);
    let title = node
        .primary_model
        .clone()
        .map(|m| short_name(&m))
        .unwrap_or_else(|| node.label.clone());

    let vram_for_share = overview_vram_gb(node.is_client, node.vram_gb);
    let share_pct = if !node.is_client && mesh_total_vram_gb > 0.0 {
        Some(((vram_for_share / mesh_total_vram_gb) * 100.0).round() as i64)
    } else {
        None
    };
    let share_label = share_pct
        .map(|p| format!("{p}%"))
        .unwrap_or_else(|| "—".to_string());

    let host_label = node.hostname.clone().unwrap_or_else(|| {
        if node.is_self {
            "this node".into()
        } else {
            "—".into()
        }
    });

    let vram_label = if node.is_client {
        "router".to_string()
    } else if node.vram_gb > 0.0 {
        format!("{:.1} GB", node.vram_gb)
    } else {
        "—".to_string()
    };

    let soc_pill = node.soc_text.clone();

    rsx! {
        div { class: "node-card-head",
            div { class: "node-card-title",
                span { class: "node-card-bullet", "•" }
                span { class: "node-card-name", "{title}" }
            }
            span { class: "badge node-card-status node-card-status--{tone}",
                "{node.status_label}"
            }
        }
        div { class: "node-card-pills",
            Pill { icon: "⌂", label: host_label }
            Pill { icon: "◇", label: format!("Role {}", node.role) }
            Pill { icon: "◧", label: format!("VRAM {vram_label}") }
            Pill { icon: "◎", label: format!("Share {share_label}") }
            if node.is_warm {
                Pill { icon: "✦", label: "Warm".to_string() }
            }
            if let Some(soc) = soc_pill {
                Pill { icon: "❖", label: soc }
            }
        }
    }
}

#[component]
fn Pill(icon: String, label: String) -> Element {
    rsx! {
        span { class: "node-pill",
            span { class: "node-pill-icon", "{icon}" }
            span { class: "node-pill-label", "{label}" }
        }
    }
}
