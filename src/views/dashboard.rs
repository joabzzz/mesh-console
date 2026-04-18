use crate::api;
use crate::process;
use crate::state::{MeshStatus, RoutingAffinity};
use crate::views::topology::TopologyView;
use dioxus::prelude::*;

#[component]
pub fn DashboardView(status: Signal<MeshStatus>, connected: Signal<bool>) -> Element {
    if !*connected.read() {
        return rsx! { StartView {} };
    }

    let st = status.read();
    let hostname = st.my_hostname.clone().unwrap_or_else(|| "—".to_string());
    let version = if st.version.is_empty() {
        "—".to_string()
    } else {
        st.version.clone()
    };
    let node_id_short = if st.node_id.len() > 16 {
        format!("{}…", &st.node_id[..16])
    } else {
        st.node_id.clone()
    };
    let role = if st.is_host {
        "host"
    } else if st.is_client {
        "client"
    } else {
        "peer"
    };
    let node_status = if st.node_status.is_empty() {
        "idle".to_string()
    } else {
        st.node_status.clone()
    };
    let inflight = st.inflight_requests;
    let api_port = st.api_port;
    let serving = st.serving_models.clone();
    let hosted = st.hosted_models.clone();
    let gpus = st.gpus.clone();
    let my_vram_gb = st.my_vram_gb;
    let model_name = st.model_name.clone();
    let configured_model_size_gb = st.model_size_gb;
    let llama_ready = st.llama_ready;
    // True online signal: this node is actually serving the configured model.
    // `model_size_gb` from the API is just the *configured* model's on-disk
    // size, set when the daemon boots; it is non-zero even when nothing is
    // loaded into VRAM. `serving_models` / `llama_ready` are the real signals.
    let model_serving = !serving.is_empty()
        || (llama_ready && !model_name.is_empty() && hosted.contains(&model_name));
    let live_vram_gb = if model_serving {
        configured_model_size_gb
    } else {
        0.0
    };
    let my_is_soc = st.my_is_soc.unwrap_or(false);
    // Invite token is consumed inside `TopologyView` via the QR action; it no
    // longer needs a dedicated row on the dashboard surface.
    let in_mesh = st.mesh_id.is_some() || st.mesh_name.is_some();
    let mesh_name = st.mesh_name.clone();
    let peers = st.peers.clone();
    let launch_pi = st.launch_pi.clone();
    let launch_goose = st.launch_goose.clone();
    let routing = st.routing_affinity.clone();
    drop(st);

    // Compute total physical VRAM across all GPUs for proportional per-bar math.
    let total_gpu_vram_bytes: u64 = gpus.iter().map(|g| g.vram_bytes).sum();

    let mesh_label = mesh_name.clone().unwrap_or_else(|| {
        if in_mesh {
            "private".to_string()
        } else {
            String::new()
        }
    });
    let _ = peers; // peer count now lives inside TopologyView

    let has_connect = launch_pi.is_some() || launch_goose.is_some();
    let has_routing = routing.is_some();

    rsx! {
        div { class: "view view--wide fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "{hostname}" }
                span { class: "badge badge--dim", "{role}" }
                span { class: "badge badge--dim", "{node_status}" }
                if !mesh_label.is_empty() {
                    span { class: "badge badge--info", "mesh · {mesh_label}" }
                }
            }

            // ── 2D topology + node detail card overlay ────────────────────
            TopologyView { status }

            ModelControl {
                model_name: model_name.clone(),
                serving: model_serving,
                llama_ready,
                model_size_gb: configured_model_size_gb,
            }

            div { class: "stat-grid",
                StatCell {
                    label: "vram in use",
                    value: format!("{live_vram_gb:.1} / {my_vram_gb:.1} GB")
                }
                StatCell { label: "in-flight", value: inflight.to_string() }
                StatCell { label: "api port", value: if api_port == 0 { "—".to_string() } else { api_port.to_string() } }
                StatCell { label: "version", value: version }
                StatCell { label: "node id", value: node_id_short }
            }

            if !gpus.is_empty() || !serving.is_empty() || !hosted.is_empty() {
                div { class: "side-by-side",
                    if !gpus.is_empty() {
                        section { class: "section side-by-side-cell",
                            h2 { class: "section-title", "GPUs" }
                            div { class: "gpu-list",
                                for gpu in gpus.iter() {
                                    {
                                        // Each GPU bar shows its physical VRAM capacity. Model
                                        // load is distributed proportionally by VRAM share.
                                        let gpu_share = if total_gpu_vram_bytes > 0 {
                                            gpu.vram_bytes as f64 / total_gpu_vram_bytes as f64
                                        } else {
                                            1.0
                                        };
                                        rsx! {
                                            GpuBar {
                                                name: gpu.name.clone(),
                                                used_gb: live_vram_gb * gpu_share,
                                                total_gb: gpu.vram_gb(),
                                                bandwidth_gbps: gpu.mem_bandwidth_gbps,
                                                is_soc: my_is_soc,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !serving.is_empty() || !hosted.is_empty() {
                        section { class: "section side-by-side-cell",
                            h2 { class: "section-title", "Models" }
                            div { class: "chip-row",
                                for m in serving.iter() {
                                    span { class: "chip chip--good", "{m}" }
                                }
                                for m in hosted.iter() {
                                    if !serving.contains(m) {
                                        span { class: "chip chip--dim", "{m}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Connect + Routing as side-by-side cards ──────────────────
            if has_connect || has_routing {
                div { class: "side-by-side",
                    if has_connect {
                        section { class: "section side-by-side-cell",
                            h2 { class: "section-title", "Connect" }
                            div { class: "info-card",
                                if let Some(ref cmd) = launch_pi {
                                    div { class: "info-row",
                                        span { class: "info-label", "pi" }
                                        div { class: "token-row",
                                            code { class: "token-value", "{cmd}" }
                                            CopyButton { value: cmd.clone() }
                                        }
                                    }
                                }
                                if let Some(ref cmd) = launch_goose {
                                    div { class: "info-row",
                                        span { class: "info-label", "goose" }
                                        div { class: "token-row",
                                            code { class: "token-value", "{cmd}" }
                                            CopyButton { value: cmd.clone() }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(ref r) = routing {
                        section { class: "section side-by-side-cell",
                            h2 { class: "section-title", "Routing" }
                            RoutingSection { r: r.clone() }
                        }
                    }
                }
            }
        }
    }
}

// ── Routing affinity section ─────────────────────────────────────────────────

#[component]
fn RoutingSection(r: RoutingAffinity) -> Element {
    let hit_rate = if r.prefix_lookups > 0 {
        format!(
            "{:.0}%",
            r.prefix_hits as f64 / r.prefix_lookups as f64 * 100.0
        )
    } else {
        "—".to_string()
    };

    rsx! {
        // Body of the Routing card; wrapping <section>/title now lives in the
        // parent so this composes with the side-by-side row layout.
        div { class: "info-card",
            div { class: "info-row",
                span { class: "info-label", "prefix" }
                if r.prefix_enabled {
                    span { class: "badge badge--green", "on" }
                } else {
                    span { class: "badge badge--dim", "off" }
                }
                span { class: "info-label", style: "margin-left: 12px;", "sticky" }
                if r.sticky_enabled {
                    span { class: "badge badge--green", "on" }
                } else {
                    span { class: "badge badge--dim", "off" }
                }
            }
        }
        div { class: "stat-grid stat-grid--compact",
            StatCell { label: "prefix routes", value: r.prefix_routes.to_string() }
            StatCell { label: "sticky routes", value: r.sticky_routes.to_string() }
            StatCell { label: "session routes", value: r.session_routes.to_string() }
            StatCell { label: "hit rate", value: hit_rate }
            StatCell { label: "learned", value: r.learned.to_string() }
            StatCell { label: "evicted", value: r.evicted.to_string() }
        }
    }
}

// ── Start view (shown when daemon is offline) ────────────────────────────────

#[component]
fn StartView() -> Element {
    let mut start_model = use_signal(String::new);
    let mut start_name = use_signal(String::new);
    let mut start_public = use_signal(|| false);
    let mut start_error = use_signal(|| Option::<String>::None);
    let mut start_pending = use_signal(|| false);

    let mut join_token = use_signal(String::new);
    let mut join_error = use_signal(|| Option::<String>::None);
    let mut join_pending = use_signal(|| false);

    rsx! {
        div { class: "view start-view fade-in",
            div { class: "start-grid",

                // ── Create a mesh ────────────────────────────────────────
                div { class: "start-card",
                    h2 { class: "start-card-title", "Start a mesh" }
                    div { class: "field",
                        label { class: "field-label", "model" }
                        input {
                            class: "field-input",
                            r#type: "text",
                            placeholder: "e.g. Qwen2.5-32B",
                            value: "{start_model}",
                            oninput: move |e| start_model.set(e.value()),
                        }
                    }
                    div { class: "field",
                        label { class: "field-label", "mesh name" }
                        input {
                            class: "field-input",
                            r#type: "text",
                            placeholder: "optional",
                            value: "{start_name}",
                            oninput: move |e| start_name.set(e.value()),
                        }
                    }
                    div { class: "field field--inline",
                        input {
                            r#type: "checkbox",
                            id: "pub-toggle",
                            checked: *start_public.read(),
                            onchange: move |e| start_public.set(e.checked()),
                        }
                        label {
                            r#for: "pub-toggle",
                            class: "field-label",
                            "public (discoverable)"
                        }
                    }
                    if let Some(ref err) = *start_error.read() {
                        div { class: "notice notice--err", "{err}" }
                    }
                    button {
                        class: "btn btn--primary",
                        disabled: *start_pending.read() || start_model.read().trim().is_empty(),
                        onclick: move |_| {
                            let model = start_model.read().trim().to_string();
                            let name = start_name.read().trim().to_string();
                            let name_opt = if name.is_empty() { None } else { Some(name) };
                            let public = *start_public.read();
                            if model.is_empty() { return; }
                            start_pending.set(true);
                            start_error.set(None);
                            match process::spawn_serve(&model, name_opt.as_deref(), public) {
                                Ok(()) => {}
                                Err(e) => { start_error.set(Some(e)); start_pending.set(false); }
                            }
                        },
                        if *start_pending.read() { "starting…" } else { "Start mesh" }
                    }
                }

                // ── Join an existing mesh ────────────────────────────────
                div { class: "start-card",
                    h2 { class: "start-card-title", "Join a mesh" }
                    div { class: "field",
                        label { class: "field-label", "invite token" }
                        input {
                            class: "field-input",
                            r#type: "text",
                            placeholder: "paste token here",
                            value: "{join_token}",
                            oninput: move |e| join_token.set(e.value()),
                        }
                    }
                    if let Some(ref err) = *join_error.read() {
                        div { class: "notice notice--err", "{err}" }
                    }
                    button {
                        class: "btn btn--primary",
                        disabled: *join_pending.read() || join_token.read().trim().is_empty(),
                        onclick: move |_| {
                            let token = join_token.read().trim().to_string();
                            if token.is_empty() { return; }
                            join_pending.set(true);
                            join_error.set(None);
                            match process::spawn_join(&token) {
                                Ok(()) => {}
                                Err(e) => { join_error.set(Some(e)); join_pending.set(false); }
                            }
                        },
                        if *join_pending.read() { "connecting…" } else { "Join" }
                    }
                }
            }
        }
    }
}

// ── Sub-components ──────────────────────────────────────────────────────────

#[component]
fn StatCell(label: String, value: String) -> Element {
    rsx! {
        div { class: "stat-cell",
            span { class: "stat-label", "{label}" }
            span { class: "stat-value", "{value}" }
        }
    }
}

/// Hero card on the Network view: shows whether the configured model is
/// online (`serving_models` contains it / `llama_ready`) and exposes
/// Start / Stop buttons that hit the `/api/runtime/models` endpoint.
#[component]
fn ModelControl(
    model_name: String,
    serving: bool,
    llama_ready: bool,
    model_size_gb: f64,
) -> Element {
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let starting = !serving && llama_ready;
    let status_label = if serving {
        "online"
    } else if starting {
        "starting"
    } else {
        "offline"
    };
    let status_class = if serving {
        "badge badge--green"
    } else if starting {
        "badge badge--dim"
    } else {
        "badge badge--muted"
    };

    rsx! {
        section { class: "section",
            h2 { class: "section-title", "Model" }
            div { class: "info-card",
                div { class: "info-row",
                    span { class: "info-label", "status" }
                    span { class: "{status_class}", "{status_label}" }
                    if model_size_gb > 0.0 {
                        span { class: "row-meta", style: "margin-left: 8px;",
                            "{model_size_gb:.1} GB on disk"
                        }
                    }
                }
                if !model_name.is_empty() {
                    div { class: "info-row",
                        span { class: "info-label", "model" }
                        span { class: "info-value", "{model_name}" }
                    }
                } else {
                    div { class: "info-row",
                        span { class: "info-label", "model" }
                        span { class: "info-value", "—" }
                        span { class: "row-meta", style: "margin-left: 8px;",
                            "start mesh-llm with --model NAME"
                        }
                    }
                }
                if !model_name.is_empty() {
                    div { class: "info-row",
                        span { class: "info-label", "" }
                        if serving {
                            button {
                                class: "btn btn--ghost btn--danger",
                                disabled: *pending.read(),
                                onclick: {
                                    let name = model_name.clone();
                                    move |_| {
                                        let n = name.clone();
                                        pending.set(true);
                                        error.set(None);
                                        async move {
                                            if let Err(e) = api::unload_model(&n).await {
                                                error.set(Some(e));
                                            }
                                            pending.set(false);
                                        }
                                    }
                                },
                                if *pending.read() { "stopping…" } else { "Stop serving" }
                            }
                        } else {
                            button {
                                class: "btn btn--primary",
                                style: "width: auto;",
                                disabled: *pending.read(),
                                onclick: {
                                    let name = model_name.clone();
                                    move |_| {
                                        let n = name.clone();
                                        pending.set(true);
                                        error.set(None);
                                        async move {
                                            if let Err(e) = api::load_model(&n).await {
                                                error.set(Some(e));
                                            }
                                            pending.set(false);
                                        }
                                    }
                                },
                                if *pending.read() { "starting…" } else { "Start serving" }
                            }
                        }
                    }
                }
                if let Some(ref msg) = *error.read() {
                    div { class: "info-hint", style: "color: rgba(248,113,113,0.85);", "{msg}" }
                }
            }
        }
    }
}

/// `used_gb`  = proportional model load for this GPU
/// `total_gb` = proportional usable VRAM ceiling for this GPU
#[component]
fn GpuBar(
    name: String,
    used_gb: f64,
    total_gb: f64,
    bandwidth_gbps: Option<f64>,
    is_soc: bool,
) -> Element {
    let pct = if total_gb > 0.0 && used_gb > 0.0 {
        (used_gb / total_gb * 100.0).min(100.0)
    } else {
        0.0
    };

    let short_name = name
        .trim_start_matches("NVIDIA GeForce ")
        .trim_start_matches("NVIDIA Quadro ")
        .trim_start_matches("NVIDIA ")
        .trim_start_matches("AMD Radeon ")
        .trim_start_matches("AMD ")
        .trim_start_matches("Apple ")
        .to_string();

    let right_label = if used_gb > 0.0 {
        format!("{used_gb:.1} / {total_gb:.1} GB")
    } else {
        format!("{total_gb:.1} GB free")
    };

    rsx! {
        div { class: "gpu-bar",
            div { class: "gpu-bar-header",
                span { class: "gpu-name",
                    "{short_name}"
                    if is_soc {
                        span { class: "badge badge--dim", style: "margin-left: 6px; font-size: 0.65rem;", "SoC" }
                    }
                }
                span { class: "gpu-meta",
                    "{right_label}"
                    if let Some(bw) = bandwidth_gbps { " · {bw:.0} GB/s" }
                }
            }
            div { class: "gpu-bar-track",
                div { class: "gpu-bar-fill", style: "width: {pct:.0}%" }
            }
        }
    }
}

#[component]
pub fn CopyButton(value: String) -> Element {
    let mut copied = use_signal(|| false);
    rsx! {
        button {
            class: if *copied.read() { "btn btn--ghost btn--sm btn--copied" } else { "btn btn--ghost btn--sm" },
            onclick: move |_| {
                let v = value.clone();
                async move {
                    let _ = document::eval(&format!("navigator.clipboard.writeText({:?})", v)).await;
                    copied.set(true);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    copied.set(false);
                }
            },
            if *copied.read() { "copied" } else { "copy" }
        }
    }
}
