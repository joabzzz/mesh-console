//! Mesh topology view: orbiting 3D cubes by default, with a 2D star fallback.
//!
//! The 3D mode renders each node as a CSS cube whose size scales with VRAM
//! and whose luminance lifts when the node is hosting a warm model. The
//! whole graph slowly orbits around the centre. Picking a cube swaps the
//! detail card shown below the stage. The 2D mode keeps the original star
//! layout where each node *is* its own mini-card.

use crate::state::{MeshStatus, PeerInfo};
use crate::status_display::{
    local_primary_model, local_status_label, mesh_total_vram_gb, overview_vram_gb,
    peer_display_name, peer_primary_model, peer_status_label, short_name, soc_label,
};
use crate::views::dashboard::CopyButton;
use dioxus::prelude::*;
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, PartialEq)]
enum TopologyMode {
    ThreeD,
    TwoD,
}

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
    /// 2D ring position in CSS pixels, relative to the centre.
    x: f64,
    y: f64,
    angle_deg: f64,
    angle_rad: f64,
    /// 3D ring position in CSS pixels. Peers sit on the XZ plane so a
    /// `rotateY` orbit around Y carries them around the centre.
    x3: f64,
    y3: f64,
    z3: f64,
    /// Cube edge length in CSS pixels for the 3D view, derived from VRAM.
    cube_px: f64,
}

/// VRAM-driven cube edge in CSS pixels. Clients and zero-VRAM nodes still
/// render at a visible minimum so they can be picked. The local node gets a
/// substantial floor bump so it reads as a big primary block even when its
/// VRAM number is small.
fn cube_size_for(vram_gb: f64, is_client: bool, is_self: bool) -> f64 {
    let min = if is_client {
        22.0
    } else if is_self {
        58.0
    } else {
        28.0
    };
    let bonus = if is_self { 12.0 } else { 0.0 };
    let scaled = if vram_gb > 0.0 {
        vram_gb.sqrt() * 9.0
    } else {
        0.0
    };
    (min + bonus + scaled).clamp(20.0, 130.0)
}

/// Short hash/identifier shown above the cube in 3D mode. Prefers hostname
/// when set, otherwise falls back to a truncated peer id.
fn cube_label(node: &SceneNode) -> String {
    let raw = node
        .hostname
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| node.id.clone());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "node".to_string()
    } else if trimmed.chars().count() > 12 {
        let truncated: String = trimmed.chars().take(10).collect();
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
            angle_rad: 0.0,
            x3: 0.0,
            y3: 0.0,
            z3: 0.0,
            cube_px: cube_size_for(vram_gb, is_client, true),
        }
    }

    fn from_peer(peer: &PeerInfo, angle_rad: f64, radius: f64, radius_3d: f64) -> Self {
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
            angle_rad,
            x3: radius_3d * angle_rad.cos(),
            y3: 0.0,
            z3: radius_3d * angle_rad.sin(),
            cube_px: cube_size_for(vram_gb, is_client, false),
        }
    }
}

fn build_scene(status: &MeshStatus, radius: f64, radius_3d: f64) -> Vec<SceneNode> {
    let mut nodes = Vec::with_capacity(1 + status.peers.len());
    if !status.node_id.is_empty() {
        nodes.push(SceneNode::from_self(status));
    }
    let n = status.peers.len() as f64;
    // Start at the top of the ring (12 o'clock) and walk clockwise.
    let start = -TAU / 4.0;
    for (i, peer) in status.peers.iter().enumerate() {
        let angle = start + (i as f64 / n.max(1.0)) * TAU;
        nodes.push(SceneNode::from_peer(peer, angle, radius, radius_3d));
    }
    for node in nodes.iter_mut() {
        node.is_warm = node_is_warm(node);
    }
    nodes
}

/// A node is "warm" — i.e. visually luminous in the 3D mesh — when it is
/// actively running inference. We derive this directly from the resolved
/// status label so the cube glow stays in sync with the per-node status
/// pill: anything labelled `Serving`, `Serving (split)`, or `Assigned`
/// (worker holding warm shards) lights up. This avoids the brittleness of
/// matching against `mesh_models[].active_nodes`, which the daemon may
/// label with hostname / short id / `"This node"` depending on context.
fn node_is_warm(node: &SceneNode) -> bool {
    matches!(
        node.status_label.as_str(),
        "Serving" | "Serving (split)" | "Assigned"
    )
}

/// 2D star ring: nudge radius up for modest peer counts so cards have a bit
/// more chord spacing. Hard-capped so the graph stays inside the fixed
/// `.topology-stage` (560px) instead of inflating the whole UI.
fn orbit_radius_2d(peer_count: usize) -> f64 {
    const CARD_WIDTH: f64 = 200.0;
    const PAD: f64 = 36.0;
    const R_MIN: f64 = 200.0;
    const R_CAP: f64 = 236.0;
    let need = CARD_WIDTH + PAD;
    match peer_count {
        0 => R_MIN,
        1 => (260.0_f64).min(R_CAP).max(R_MIN),
        n => {
            let n = n as f64;
            let r = need / (2.0 * (PI / n).sin().max(1e-6));
            r.max(R_MIN).min(R_CAP)
        }
    }
}

/// 3D orbit: same idea — a touch more radius when it helps, capped for layout.
fn orbit_radius_3d(peer_count: usize) -> f64 {
    const NODE_CLEAR: f64 = 152.0;
    const PAD: f64 = 32.0;
    const R_MIN: f64 = 220.0;
    const R_CAP: f64 = 258.0;
    let need = NODE_CLEAR + PAD;
    match peer_count {
        0 => R_MIN,
        1 => (280.0_f64).min(R_CAP).max(R_MIN),
        n => {
            let n = n as f64;
            let r = need / (2.0 * (PI / n).sin().max(1e-6));
            r.max(R_MIN).min(R_CAP)
        }
    }
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
    let radius_2d = orbit_radius_2d(peer_count);
    let radius_3d = orbit_radius_3d(peer_count);
    let scene = build_scene(&st, radius_2d, radius_3d);
    let total_vram = mesh_total_vram_gb(&st);
    let token = st.token.clone();
    let self_id = st.node_id.clone();
    drop(st);

    let stage_3d_orbit_var = format!("--topology-orbit-r: {:.0}px;", radius_3d);

    let mut mode = use_signal(|| TopologyMode::ThreeD);
    let mut selected_id = use_signal(String::new);
    let mut hover_id = use_signal(String::new);
    let current_selection = selected_id.read().clone();
    let current_hover = hover_id.read().clone();
    // Visible selection ring + bottom detail card both follow `displayed_id`:
    // hover wins so moving the mouse around previews each cube; click pins
    // a selection that persists once the mouse leaves the orbit.
    let displayed_id = if !current_hover.is_empty() {
        current_hover
    } else if !current_selection.is_empty() {
        current_selection
    } else {
        self_id.clone()
    };
    let current_mode = *mode.read();
    let displayed_node = scene.iter().find(|n| n.id == displayed_id).cloned();

    rsx! {
        section { class: "section",
            div { class: "section-header-row",
                div { class: "section-title-row",
                    h2 { class: "section-title", "Network" }
                    span { class: "section-count", "{1 + peer_count} nodes" }
                }
                div { class: "topology-controls",
                    div {
                        class: "topology-mode-toggle",
                        role: "group",
                        "aria-label": "Topology view mode",
                        button {
                            class: if matches!(current_mode, TopologyMode::ThreeD) {
                                "topology-mode-btn topology-mode-btn--active"
                            } else {
                                "topology-mode-btn"
                            },
                            onclick: move |_| mode.set(TopologyMode::ThreeD),
                            title: "Orbiting 3D view",
                            "3D"
                        }
                        button {
                            class: if matches!(current_mode, TopologyMode::TwoD) {
                                "topology-mode-btn topology-mode-btn--active"
                            } else {
                                "topology-mode-btn"
                            },
                            onclick: move |_| mode.set(TopologyMode::TwoD),
                            title: "Flat 2D star view",
                            "2D"
                        }
                    }
                    if !token.is_empty() {
                        QrInviteButton { token: token.clone(), label: "QR invite".to_string() }
                    }
                }
            }

            match current_mode {
                TopologyMode::TwoD => rsx! {
                    div { class: "topology-stage",
                        div { class: "topology-scene",
                            for node in scene.iter().filter(|n| !n.is_self) {
                                SpokeLine {
                                    angle_deg: node.angle_deg,
                                    length: radius_2d,
                                    tone: status_tone(&node.status_label),
                                }
                            }
                            for node in scene.iter() {
                                NodeCardMarker {
                                    key: "{node.id}",
                                    node: node.clone(),
                                    mesh_total_vram_gb: total_vram,
                                    selected: displayed_id == node.id,
                                    on_select: {
                                        let id = node.id.clone();
                                        move |_| selected_id.set(id.clone())
                                    },
                                }
                            }
                        }
                    }
                },
                TopologyMode::ThreeD => rsx! {
                    div {
                        class: "topology-stage topology-stage--3d",
                        style: "{stage_3d_orbit_var}",
                        div { class: "topology-3d-perspective",
                            div { class: "topology-3d-tilt",
                                div { class: "topology-3d-spin",
                                    // Faint spokes from the host (origin) out to
                                    // each peer in the XZ orbit plane.
                                    for node in scene.iter().filter(|n| !n.is_self) {
                                        TopologySpoke3D {
                                            angle_rad: node.angle_rad,
                                            length: radius_3d,
                                            tone: status_tone(&node.status_label),
                                        }
                                    }
                                    for node in scene.iter() {
                                        TopologyCube {
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
                        }
                        div { class: "topology-3d-ground" }
                    }
                    if let Some(node) = displayed_node {
                        div { class: "topology-detail-row",
                            NodeCardDetail {
                                node,
                                mesh_total_vram_gb: total_vram,
                            }
                        }
                    }
                    div { class: "topology-node-roster",
                        p { class: "topology-node-roster-heading", "All nodes" }
                        div { class: "topology-node-roster-rows",
                            for node in scene.iter() {
                                TopologyRosterRow {
                                    key: "{node.id}",
                                    node: node.clone(),
                                    selected: displayed_id == node.id,
                                    on_select: {
                                        let id = node.id.clone();
                                        move |_| selected_id.set(id.clone())
                                    },
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// One scannable row in the 3D-mode roster: full node id, host, role, status,
/// and warm flag — fields the orbit detail card can omit or truncate.
#[component]
fn TopologyRosterRow(node: SceneNode, selected: bool, on_select: EventHandler<()>) -> Element {
    let tone = status_tone(&node.status_label);
    let mut row_class = String::from("topology-roster-row");
    if selected {
        row_class.push_str(" topology-roster-row--selected");
    }
    if node.is_warm {
        row_class.push_str(" topology-roster-row--warm");
    }

    let host_line = match &node.hostname {
        Some(h) if !h.trim().is_empty() => format!("Host · {}", h.trim()),
        _ if node.is_self => "Host · this machine".to_string(),
        _ => "Host · —".to_string(),
    };

    let role_vram = if node.is_client {
        format!("Role · {} · router", node.role)
    } else if node.vram_gb > 0.0 {
        format!("Role · {} · {:.1} GB VRAM", node.role, node.vram_gb)
    } else {
        format!("Role · {} · —", node.role)
    };

    rsx! {
        button {
            class: "{row_class}",
            onclick: move |_| on_select.call(()),
            div { class: "topology-roster-main",
                div { class: "topology-roster-title-line",
                    span { class: "topology-roster-name", "{node.label}" }
                    if node.is_self {
                        span { class: "topology-roster-you", "You" }
                    }
                }
                span { class: "topology-roster-id", "{node.id}" }
                span { class: "topology-roster-meta", "{host_line}" }
                span { class: "topology-roster-meta", "{role_vram}" }
                if let Some(ref m) = node.primary_model {
                    span { class: "topology-roster-meta", "Model · {short_name(m)}" }
                }
            }
            div { class: "topology-roster-side",
                span { class: "badge node-card-status node-card-status--{tone}",
                    "{node.status_label}"
                }
                div { class: "topology-roster-badges",
                    if node.is_warm {
                        span { class: "topology-roster-chip topology-roster-chip--warm", "Warm" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn QrInviteButton(token: String, label: String) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        button {
            class: "btn btn--ghost btn--sm",
            onclick: move |_| open.set(true),
            title: "Show invite QR",
            "{label}"
        }
        if *open.read() {
            QrModal {
                token: token.clone(),
                on_close: move |_| open.set(false),
            }
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

/// Faint 3D edge from the centre (host) out to a peer cube. The bar is laid
/// along +X and rotated around Y so it lies flat in the orbit's XZ plane.
/// CSS rotateY rotates clockwise when looking down +Y; the peer position is
/// `(R*cos(a), 0, R*sin(a))`, so we rotate by `-a` to point the bar at it.
#[component]
fn TopologySpoke3D(angle_rad: f64, length: f64, tone: String) -> Element {
    let style = format!(
        "transform: rotateY({angle:.4}rad); width: {len:.2}px;",
        angle = -angle_rad,
        len = length,
    );
    rsx! {
        div {
            class: "topology-3d-spoke topology-3d-spoke--{tone}",
            style: "{style}",
        }
    }
}

/// 2D view marker: each node renders as a self-contained mini-card sitting
/// at its ring position. There is no separate "selected" card — the cards
/// *are* the topology.
#[component]
fn NodeCardMarker(
    node: SceneNode,
    mesh_total_vram_gb: f64,
    selected: bool,
    on_select: EventHandler<()>,
) -> Element {
    let tone = status_tone(&node.status_label);
    let mut card_classes = vec!["node-card", "node-card-marker"];
    let tone_modifier = format!("node-card--{tone}");
    card_classes.push(&tone_modifier);
    if node.is_self {
        card_classes.push("node-card-marker--self");
    }
    if selected {
        card_classes.push("node-card-marker--selected");
    }
    let class_attr = card_classes.join(" ");

    let position_style = format!("transform: translate({:.2}px, {:.2}px);", node.x, node.y);

    rsx! {
        button {
            class: "{class_attr}",
            style: "{position_style}",
            onclick: move |_| on_select.call(()),
            NodeCardBody { node: node.clone(), mesh_total_vram_gb }
        }
    }
}

/// 3D view detail card: same content as the 2D marker, rendered as a plain
/// card under the orbit stage so the selected node still shows full info.
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

/// Shared head + pill row used by both the 2D marker and the 3D detail card.
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

/// 3D view marker: a CSS cube whose edge length tracks VRAM and whose face
/// luminance lifts when the node is warm. Acts as the click target and the
/// hover preview source for the detail card below the orbit stage.
#[component]
fn TopologyCube(
    node: SceneNode,
    selected: bool,
    on_select: EventHandler<()>,
    on_hover_enter: EventHandler<()>,
    on_hover_leave: EventHandler<()>,
) -> Element {
    let tone = status_tone(&node.status_label);
    let mut classes = vec![
        "topology-cube".to_string(),
        format!("topology-cube--{tone}"),
    ];
    if node.is_self {
        classes.push("topology-cube--self".to_string());
    }
    if node.is_warm {
        classes.push("topology-cube--warm".to_string());
    }
    if node.is_client {
        classes.push("topology-cube--client".to_string());
    }
    if selected {
        classes.push("topology-cube--selected".to_string());
    }
    let class_attr = classes.join(" ");

    let half = node.cube_px / 2.0;
    // Per-cube isometric tilt so 3 faces are *always* visible — without this
    // the host cube (which sits at the rotation origin) looks like a flat
    // square whenever the orbit happens to align it head-on with the camera.
    // Transform reads right-to-left:
    //   1. translate(-50%, -50%) — anchor the cube's top-left to its centre
    //   2. rotateX/rotateY      — tilt it isometrically around its own centre
    //   3. translate3d(...)     — move it to its world position in the orbit
    let style = format!(
        "width: {sz:.1}px; height: {sz:.1}px; --topology-cube-half: {half:.1}px; \
         transform: translate3d({x:.2}px, {y:.2}px, {z:.2}px) \
         rotateX(-18deg) rotateY(28deg) translate(-50%, -50%);",
        sz = node.cube_px,
        half = half,
        x = node.x3,
        y = node.y3,
        z = node.z3,
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

    let label_text = cube_label(&node);
    let mut label_classes = vec!["topology-cube-label".to_string()];
    if node.is_self {
        label_classes.push("topology-cube-label--self".to_string());
    }
    if node.is_warm {
        label_classes.push("topology-cube-label--warm".to_string());
    }
    let label_class_attr = label_classes.join(" ");

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
            div { class: "topology-cube-face topology-cube-face--front" }
            div { class: "topology-cube-face topology-cube-face--back" }
            div { class: "topology-cube-face topology-cube-face--right" }
            div { class: "topology-cube-face topology-cube-face--left" }
            div { class: "topology-cube-face topology-cube-face--top" }
            div { class: "topology-cube-face topology-cube-face--bottom" }
            div { class: "{label_class_attr}", "{label_text}" }
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

#[component]
fn QrModal(token: String, on_close: EventHandler<()>) -> Element {
    let payload = format!("mesh-llm serve --join {token}");
    let svg = qrcode::QrCode::new(payload.as_bytes())
        .map(|code| {
            code.render::<qrcode::render::svg::Color<'_>>()
                .min_dimensions(220, 220)
                .quiet_zone(true)
                .dark_color(qrcode::render::svg::Color("#0c0d10"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_else(|_| String::from("<p>Failed to encode token.</p>"));

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal-card",
                onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h3 { class: "modal-title", "Invite QR" }
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |_| on_close.call(()),
                        "close"
                    }
                }
                div { class: "qr-frame",
                    div { class: "qr-svg", dangerous_inner_html: "{svg}" }
                }
                p { class: "modal-hint",
                    "Scan to copy the join command, or paste the token directly:"
                }
                div { class: "token-row",
                    code { class: "token-value",
                        "mesh-llm serve --join {&token[..token.len().min(48)]}…"
                    }
                    CopyButton { value: token.clone() }
                }
            }
        }
    }
}
