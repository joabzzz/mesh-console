use crate::api;
use crate::process;
use crate::state::{DiscoveredMesh, MeshStatus};
use crate::views::dashboard::CopyButton;
use dioxus::prelude::*;

#[component]
pub fn NetworksView(status: Signal<MeshStatus>, connected: Signal<bool>) -> Element {
    if !*connected.read() {
        return rsx! {
            div { class: "view fade-in",
                header { class: "view-header",
                    h1 { class: "view-title", "Networks" }
                }
                div { class: "empty-state",
                    span { "Daemon is offline" }
                    span { class: "empty-hint", "Start a mesh from the Dashboard to get going." }
                }
            }
        };
    }

    let mut meshes = use_signal(Vec::<DiscoveredMesh>::new);
    let mut discover_loading = use_signal(|| false);
    let mut discover_error = use_signal(|| Option::<String>::None);
    let mut stop_error = use_signal(|| Option::<String>::None);
    let mut show_create = use_signal(|| false);

    use_future(move || async move {
        discover_loading.set(true);
        match api::fetch_discover().await {
            Ok(list) => { meshes.set(list); discover_error.set(None); }
            Err(e) => discover_error.set(Some(e)),
        }
        discover_loading.set(false);
    });

    let st = status.read();
    let token = st.token.clone();
    let mesh_name = st.mesh_name.clone();
    let nostr_public = st.nostr_discovery;
    let peer_count = st.peers.len();
    let serving: Vec<String> = {
        let mut m = st.serving_models.clone();
        m.extend(st.hosted_models.clone());
        m.dedup();
        m
    };
    drop(st);

    rsx! {
        div { class: "view fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "Networks" }
            }

            // ── This node's mesh ─────────────────────────────────────────
            section { class: "section",
                div { class: "section-header-row",
                    h2 { class: "section-title", "This Node" }
                    button {
                        class: "btn btn--ghost btn--sm btn--danger",
                        onclick: move |_| {
                            stop_error.set(None);
                            match process::stop_daemon() {
                                Ok(()) => {}
                                Err(e) => stop_error.set(Some(e)),
                            }
                        },
                        "Stop mesh"
                    }
                }

                if let Some(ref err) = *stop_error.read() {
                    div { class: "notice notice--err", "{err}" }
                }

                div { class: "info-card",
                    div { class: "info-row",
                        span { class: "info-label", "name" }
                        span { class: "info-value",
                            if let Some(ref n) = mesh_name { "{n}" } else { "private" }
                        }
                    }
                    div { class: "info-row",
                        span { class: "info-label", "visibility" }
                        if nostr_public {
                            span { class: "badge badge--green", "public" }
                        } else {
                            span { class: "badge badge--dim", "private" }
                        }
                    }
                    div { class: "info-row",
                        span { class: "info-label", "peers" }
                        span { class: "info-value", "{peer_count}" }
                    }
                    if !serving.is_empty() {
                        div { class: "info-row",
                            span { class: "info-label", "models" }
                            div { class: "cap-row",
                                for m in serving.iter() {
                                    span { class: "badge badge--green", "{m}" }
                                }
                            }
                        }
                    }
                    if !token.is_empty() {
                        div { class: "info-row",
                            span { class: "info-label", "invite token" }
                            div { class: "token-row",
                                code { class: "token-value",
                                    "{&token[..token.len().min(52)]}…"
                                }
                                CopyButton { value: token.clone() }
                            }
                        }
                    }
                }
            }

            // ── Public networks ──────────────────────────────────────────
            section { class: "section",
                div { class: "section-header-row",
                    h2 { class: "section-title", "Public Networks" }
                    button {
                        class: "btn btn--ghost btn--sm",
                        disabled: *discover_loading.read(),
                        onclick: move |_| async move {
                            if *discover_loading.read() { return; }
                            discover_loading.set(true);
                            match api::fetch_discover().await {
                                Ok(list) => { meshes.set(list); discover_error.set(None); }
                                Err(e) => discover_error.set(Some(e)),
                            }
                            discover_loading.set(false);
                        },
                        if *discover_loading.read() { "scanning…" } else { "refresh" }
                    }
                }

                if let Some(ref err) = *discover_error.read() {
                    div { class: "notice notice--err", "{err}" }
                }

                if meshes.read().is_empty() && !*discover_loading.read() {
                    div { class: "empty-state",
                        span { "No public networks found" }
                        span { class: "empty-hint", "Public meshes published via Nostr will appear here." }
                    }
                } else {
                    div { class: "row-list",
                        for mesh in meshes.read().iter() {
                            MeshCard {
                                mesh: mesh.clone(),
                                currently_connected: true,
                            }
                        }
                    }
                }
            }

            // ── Start a new mesh ─────────────────────────────────────────
            section { class: "section",
                div { class: "section-header-row",
                    h2 { class: "section-title", "Start a new mesh" }
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |_| { let v = *show_create.read(); show_create.set(!v); },
                        if *show_create.read() { "hide" } else { "+" }
                    }
                }
                if *show_create.read() {
                    div { class: "notice notice--warn",
                        "This will stop your current mesh before starting a new one."
                    }
                    CreateMeshPanel { stop_first: true }
                }
            }
        }
    }
}

// ── Mesh card ────────────────────────────────────────────────────────────────

#[component]
fn MeshCard(mesh: DiscoveredMesh, currently_connected: bool) -> Element {
    let listing = mesh.listing.clone();
    let name = listing.name.clone().unwrap_or_else(|| "unnamed".to_string());
    let vram = listing.total_vram_gb();
    let nodes = listing.node_count;
    let region = listing.region.clone();
    let serving = listing.serving.clone();
    let invite = listing.invite_token.clone();

    let mut joining = use_signal(|| false);
    let mut join_error = use_signal(|| Option::<String>::None);
    let mut confirm = use_signal(|| false);

    rsx! {
        div { class: "row-item row-item--mesh",
            div { class: "row-main",
                div { class: "peer-header",
                    span { class: "row-name", "{name}" }
                    if let Some(ref r) = region {
                        span { class: "badge badge--dim", "{r}" }
                    }
                }
                div { class: "peer-meta",
                    span { class: "row-meta", "{nodes} nodes" }
                    span { class: "row-meta", "{vram:.0} GB VRAM" }
                    if listing.client_count > 0 {
                        span { class: "row-meta", "{listing.client_count} clients" }
                    }
                }
                if !serving.is_empty() {
                    div { class: "cap-row",
                        for m in serving.iter() {
                            span { class: "badge badge--dim", "{m}" }
                        }
                    }
                }
                if let Some(ref err) = *join_error.read() {
                    div { class: "notice notice--err", "{err}" }
                }
            }
            div { class: "row-actions",
                if currently_connected && !*confirm.read() {
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |_| confirm.set(true),
                        "Join"
                    }
                } else if currently_connected && *confirm.read() {
                    div { class: "confirm-row",
                        span { class: "confirm-label", "Stop current & join?" }
                        button {
                            class: "btn btn--ghost btn--sm btn--danger",
                            disabled: *joining.read(),
                            onclick: {
                                let token = invite.clone();
                                move |_| {
                                    let t = token.clone();
                                    join_error.set(None);
                                    joining.set(true);
                                    let stop_result = process::stop_daemon();
                                    async move {
                                        if let Err(e) = stop_result {
                                            join_error.set(Some(e));
                                            joining.set(false);
                                            confirm.set(false);
                                            return;
                                        }
                                        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                                        if let Err(e) = process::spawn_join(&t) {
                                            join_error.set(Some(e));
                                        }
                                        joining.set(false);
                                        confirm.set(false);
                                    }
                                }
                            },
                            if *joining.read() { "joining…" } else { "Yes, leave & join" }
                        }
                        button {
                            class: "btn btn--ghost btn--sm",
                            onclick: move |_| confirm.set(false),
                            "Cancel"
                        }
                    }
                } else {
                    button {
                        class: "btn btn--ghost btn--sm",
                        disabled: *joining.read(),
                        onclick: {
                            let token = invite.clone();
                            move |_| {
                                let t = token.clone();
                                join_error.set(None);
                                joining.set(true);
                                match process::spawn_join(&t) {
                                    Ok(()) => {}
                                    Err(e) => { join_error.set(Some(e)); joining.set(false); }
                                }
                            }
                        },
                        if *joining.read() { "joining…" } else { "Join" }
                    }
                }
            }
        }
    }
}

// ── Create mesh panel ────────────────────────────────────────────────────────

#[component]
fn CreateMeshPanel(stop_first: bool) -> Element {
    let mut model = use_signal(String::new);
    let mut mesh_name = use_signal(String::new);
    let mut is_public = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut pending = use_signal(|| false);

    rsx! {
        div { class: "create-panel",
            div { class: "create-fields",
                div { class: "field",
                    label { class: "field-label", "model" }
                    input {
                        class: "field-input",
                        r#type: "text",
                        placeholder: "e.g. Qwen2.5-32B",
                        value: "{model}",
                        oninput: move |e| model.set(e.value()),
                    }
                }
                div { class: "field",
                    label { class: "field-label", "mesh name" }
                    input {
                        class: "field-input",
                        r#type: "text",
                        placeholder: "optional",
                        value: "{mesh_name}",
                        oninput: move |e| mesh_name.set(e.value()),
                    }
                }
                div { class: "field field--inline",
                    input {
                        r#type: "checkbox",
                        id: "new-pub-toggle",
                        checked: *is_public.read(),
                        onchange: move |e| is_public.set(e.checked()),
                    }
                    label { r#for: "new-pub-toggle", class: "field-label", "public (discoverable)" }
                }
            }
            if let Some(ref err) = *error.read() {
                div { class: "notice notice--err", "{err}" }
            }
            div { class: "create-actions",
                button {
                    class: "btn btn--primary",
                    disabled: *pending.read() || model.read().trim().is_empty(),
                    onclick: move |_| {
                        let m = model.read().trim().to_string();
                        let n = mesh_name.read().trim().to_string();
                        let n_opt = if n.is_empty() { None } else { Some(n) };
                        let pub_flag = *is_public.read();
                        if m.is_empty() { return; }
                        error.set(None);
                        pending.set(true);
                        if stop_first {
                            let _ = process::stop_daemon();
                        }
                        match process::spawn_serve(&m, n_opt.as_deref(), pub_flag) {
                            Ok(()) => {}
                            Err(e) => { error.set(Some(e)); pending.set(false); }
                        }
                    },
                    if *pending.read() { "starting…" } else { "Start mesh" }
                }
            }
        }
    }
}
