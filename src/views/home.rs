//! Home tab — the landing surface for the console.
//!
//! Three sections:
//!   1. **Active on this device** — local mesh-llm daemons (`MeshStatus.local_instances`)
//!      plus the currently-attached mesh summary when we're connected.
//!   2. **Public networks** — meshes published over Nostr, fetched via
//!      `/api/discover`.
//!   3. **+ button** — opens an inline panel that spawns
//!      `mesh-llm serve --model … [--mesh-name …] [--publish]`.
//!
//! Discovery and local-instance enumeration live behind the management HTTP
//! API on `:3131`; the public Rust SDK ([`mesh_api`]) doesn't expose them.
//! Where the SDK *can* help we use it: invite tokens are parsed through
//! [`crate::sdk_session::validate_invite_token`] (so the Join button rejects
//! malformed tokens before we fork a subprocess), and the device identity
//! row shows the SDK-derived owner id.

use crate::api;
use crate::process;
use crate::sdk_session;
use crate::state::{DiscoveredMesh, LocalInstance, MeshStatus};
use crate::views::dashboard::CopyButton;
use chrono::{TimeZone, Utc};
use dioxus::prelude::*;

#[component]
pub fn HomeView(
    status: Signal<MeshStatus>,
    connected: Signal<bool>,
    /// Fired when the user opens the active mesh card. The parent (App in
    /// `main.rs`) routes this to the Network tab so `TopologyView` can show
    /// the live 2D/3D graph for the currently-attached mesh.
    on_view_network: EventHandler<()>,
) -> Element {
    let mut meshes = use_signal(Vec::<DiscoveredMesh>::new);
    let mut discover_loading = use_signal(|| false);
    let mut discover_error = use_signal(|| Option::<String>::None);
    let mut show_create = use_signal(|| false);
    let mut owner = use_signal(|| Option::<String>::None);

    // Owner id only — comes from the local keypair file, not the daemon, so
    // it loads even when mesh-llm isn't running.
    use_future(move || async move {
        if owner.read().is_none() {
            if let Ok(id) = tokio::task::spawn_blocking(sdk_session::owner_id)
                .await
                .unwrap_or_else(|e| Err(format!("owner task: {e}")))
            {
                owner.set(Some(id));
            }
        }
    });

    // Discover only when the daemon is up. Hitting `/api/discover` while
    // mesh-llm is offline produces a connect-refused error that's not
    // actionable from the Home tab — the user just hasn't started a mesh
    // yet. Re-fetch automatically on every reconnect.
    use_effect(move || {
        let is_connected = *connected.read();
        if !is_connected {
            meshes.set(Vec::new());
            discover_error.set(None);
            discover_loading.set(false);
            return;
        }
        spawn(async move {
            discover_loading.set(true);
            match api::fetch_discover().await {
                Ok(list) => {
                    meshes.set(list);
                    discover_error.set(None);
                }
                Err(e) => discover_error.set(Some(e)),
            }
            discover_loading.set(false);
        });
    });

    // Snapshot the bits of MeshStatus we care about up-front so the closures
    // below don't need to keep the signal borrowed.
    let st = status.read();
    let attached_mesh_name = st.mesh_name.clone();
    let attached_mesh_id = st.mesh_id.clone();
    let attached_public = st.nostr_discovery;
    let attached_peers = st.peers.len();
    let attached_models: Vec<String> = {
        let mut m = st.serving_models.clone();
        for h in &st.hosted_models {
            if !m.contains(h) {
                m.push(h.clone());
            }
        }
        m
    };
    let local_instances = st.local_instances.clone();
    drop(st);

    let owner_label = owner
        .read()
        .clone()
        .map(|id| {
            if id.len() > 16 {
                format!("{}…", &id[..16])
            } else {
                id
            }
        })
        .unwrap_or_else(|| "—".to_string());

    let connected_now = *connected.read();

    rsx! {
        div { class: "view view--wide fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "Home" }
                span { class: "badge badge--dim", "owner · {owner_label}" }
            }

            // ── Section 1: active on this device ────────────────────────
            section { class: "section",
                div { class: "section-header-row",
                    h2 { class: "section-title", "Active on this device" }
                }

                if local_instances.is_empty() && !connected_now {
                    div { class: "empty-state",
                        span { "No mesh-llm daemon running" }
                        span { class: "empty-hint", "Tap + below to start a mesh." }
                    }
                } else {
                    div { class: "row-list",
                        // Currently-attached mesh as the headline card. We
                        // render this even if the daemon hasn't reported
                        // local_instances yet, so users always see what
                        // they're connected to.
                        if connected_now {
                            ActiveMeshCard {
                                mesh_name: attached_mesh_name.clone(),
                                mesh_id: attached_mesh_id.clone(),
                                is_public: attached_public,
                                peer_count: attached_peers,
                                models: attached_models.clone(),
                                on_open: move |_| on_view_network.call(()),
                            }
                        }
                        for inst in local_instances.iter() {
                            LocalInstanceRow { inst: inst.clone() }
                        }
                    }
                }
            }

            // ── Section 2: public networks ──────────────────────────────
            section { class: "section",
                div { class: "section-header-row",
                    h2 { class: "section-title", "Public networks" }
                    if connected_now {
                        button {
                            class: "btn btn--ghost btn--sm",
                            disabled: *discover_loading.read(),
                            onclick: move |_| async move {
                                if *discover_loading.read() { return; }
                                discover_loading.set(true);
                                match api::fetch_discover().await {
                                    Ok(list) => {
                                        meshes.set(list);
                                        discover_error.set(None);
                                    }
                                    Err(e) => discover_error.set(Some(e)),
                                }
                                discover_loading.set(false);
                            },
                            if *discover_loading.read() { "scanning…" } else { "refresh" }
                        }
                    }
                }

                if !connected_now {
                    div { class: "empty-state",
                        span { "Daemon offline" }
                        span { class: "empty-hint", "Start a mesh below to scan for public networks via Nostr." }
                    }
                } else if let Some(ref err) = *discover_error.read() {
                    div { class: "notice notice--err", "{err}" }
                } else if meshes.read().is_empty() && !*discover_loading.read() {
                    div { class: "empty-state",
                        span { "No public networks found" }
                        span { class: "empty-hint", "Public meshes published via Nostr will appear here." }
                    }
                } else {
                    div { class: "row-list",
                        for mesh in meshes.read().iter() {
                            PublicMeshCard {
                                mesh: mesh.clone(),
                                attached: connected_now,
                            }
                        }
                    }
                }
            }

            // ── Section 3: floating "+" → start a mesh ──────────────────
            section { class: "section",
                div { class: "section-header-row",
                    h2 { class: "section-title", "Start a new mesh" }
                    button {
                        class: if *show_create.read() { "btn btn--ghost btn--sm" } else { "home-fab" },
                        title: "Start a new mesh",
                        onclick: move |_| { let v = *show_create.read(); show_create.set(!v); },
                        if *show_create.read() { "hide" } else { "+" }
                    }
                }
                if *show_create.read() {
                    if connected_now {
                        div { class: "notice notice--warn",
                            "Starting a new mesh stops the daemon currently running on this device."
                        }
                    }
                    CreateMeshPanel { stop_first: connected_now }
                }
            }
        }
    }
}

// ── Section 1 components ────────────────────────────────────────────────────

#[component]
fn ActiveMeshCard(
    mesh_name: Option<String>,
    mesh_id: Option<String>,
    is_public: bool,
    peer_count: usize,
    models: Vec<String>,
    /// Fired when the user clicks anywhere on the card outside an action
    /// button. The Home view routes this to the Network tab so the user
    /// lands on `TopologyView` for the live mesh.
    on_open: EventHandler<()>,
) -> Element {
    let display_name = mesh_name
        .clone()
        .unwrap_or_else(|| "private mesh".to_string());
    let mut stop_error = use_signal(|| Option::<String>::None);

    rsx! {
        div {
            class: "row-item row-item--mesh row-item--clickable",
            onclick: move |_| on_open.call(()),
            title: "Open topology",
            div { class: "row-main",
                div { class: "peer-header",
                    span { class: "row-name", "{display_name}" }
                    span { class: "badge badge--info", "this device" }
                    if is_public {
                        span { class: "badge badge--green", "public" }
                    } else {
                        span { class: "badge badge--dim", "private" }
                    }
                }
                div { class: "peer-meta",
                    span { class: "row-meta", "{peer_count} peers" }
                    if let Some(ref id) = mesh_id {
                        span { class: "row-meta",
                            "id "
                            code { "{&id[..id.len().min(12)]}" }
                        }
                    }
                    span { class: "row-meta row-meta--hint", "click to open topology →" }
                }
                if !models.is_empty() {
                    div { class: "cap-row",
                        for m in models.iter() {
                            span { class: "badge badge--green", "{m}" }
                        }
                    }
                }
                if let Some(ref err) = *stop_error.read() {
                    div { class: "notice notice--err", "{err}" }
                }
            }
            div {
                class: "row-actions",
                onclick: move |evt: Event<MouseData>| evt.stop_propagation(),
                button {
                    class: "btn btn--ghost btn--sm btn--danger",
                    onclick: move |_| {
                        stop_error.set(None);
                        if let Err(e) = process::stop_daemon() {
                            stop_error.set(Some(e));
                        }
                    },
                    "Stop"
                }
            }
        }
    }
}

#[component]
fn LocalInstanceRow(inst: LocalInstance) -> Element {
    // Skip the "self" instance — it's already represented by ActiveMeshCard
    // so we don't double-render the same daemon.
    if inst.is_self {
        return rsx! {};
    }

    let mut stop_error = use_signal(|| Option::<String>::None);
    let mut stopping = use_signal(|| false);

    let started = if inst.started_at_unix > 0 {
        Utc.timestamp_opt(inst.started_at_unix, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "—".to_string())
    } else {
        "—".to_string()
    };
    let port = inst
        .api_port
        .map(|p| p.to_string())
        .unwrap_or_else(|| "—".to_string());
    let version = inst.version.clone().unwrap_or_else(|| "—".to_string());
    let pid = inst.pid;

    rsx! {
        div { class: "row-item row-item--mesh",
            div { class: "row-main",
                div { class: "peer-header",
                    span { class: "row-name", "mesh-llm pid {pid}" }
                    span { class: "badge badge--dim", "local" }
                }
                div { class: "peer-meta",
                    span { class: "row-meta", "port {port}" }
                    span { class: "row-meta", "v{version}" }
                    span { class: "row-meta", "started {started}" }
                }
                if let Some(ref err) = *stop_error.read() {
                    div { class: "notice notice--err", "{err}" }
                }
            }
            div { class: "row-actions",
                button {
                    class: "btn btn--ghost btn--sm btn--danger",
                    disabled: *stopping.read(),
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        stop_error.set(None);
                        stopping.set(true);
                        let res = process::stop_pid(pid);
                        if let Err(e) = res {
                            stop_error.set(Some(e));
                        }
                        stopping.set(false);
                    },
                    if *stopping.read() { "stopping…" } else { "Stop" }
                }
            }
        }
    }
}

// ── Section 2 components ────────────────────────────────────────────────────

#[component]
fn PublicMeshCard(mesh: DiscoveredMesh, attached: bool) -> Element {
    let listing = mesh.listing.clone();
    let name = listing
        .name
        .clone()
        .unwrap_or_else(|| "unnamed".to_string());
    let vram = listing.total_vram_gb();
    let nodes = listing.node_count;
    let region = listing.region.clone();
    let serving = listing.serving.clone();
    let wanted = listing.wanted.clone();
    let on_disk = listing.on_disk.clone();
    let invite = listing.invite_token.clone();
    let mesh_id = listing.mesh_id.clone();
    let publisher = mesh.publisher_npub.clone();
    let published_at = mesh.published_at;
    let expires_at = mesh.expires_at;

    let mut expanded = use_signal(|| false);
    let mut joining = use_signal(|| false);
    let mut join_error = use_signal(|| Option::<String>::None);
    let mut confirm = use_signal(|| false);

    let published_label = if published_at > 0 {
        Utc.timestamp_opt(published_at as i64, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "—".to_string())
    } else {
        "—".to_string()
    };
    let expires_label = expires_at.and_then(|ts| {
        Utc.timestamp_opt(ts as i64, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
    });

    rsx! {
        div {
            class: "row-item row-item--mesh row-item--clickable",
            onclick: move |_| { let v = *expanded.read(); expanded.set(!v); },
            title: if *expanded.read() { "Hide details" } else { "Show details" },
            div { class: "row-main",
                div { class: "peer-header",
                    span { class: "row-name", "{name}" }
                    if let Some(ref r) = region {
                        span { class: "badge badge--dim", "{r}" }
                    }
                    span { class: "row-meta row-meta--hint",
                        if *expanded.read() { "▾" } else { "▸" }
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

                // ── Expanded detail panel ────────────────────────────────
                if *expanded.read() {
                    div { class: "info-card",
                        if let Some(ref id) = mesh_id {
                            div { class: "info-row",
                                span { class: "info-label", "mesh id" }
                                span { class: "info-value",
                                    code { "{id}" }
                                }
                            }
                        }
                        if !publisher.is_empty() {
                            div { class: "info-row",
                                span { class: "info-label", "publisher" }
                                div { class: "token-row",
                                    code { class: "token-value",
                                        "{&publisher[..publisher.len().min(48)]}"
                                        if publisher.len() > 48 { "…" }
                                    }
                                    CopyButton { value: publisher.clone() }
                                }
                            }
                        }
                        div { class: "info-row",
                            span { class: "info-label", "published" }
                            span { class: "info-value", "{published_label}" }
                        }
                        if let Some(ref exp) = expires_label {
                            div { class: "info-row",
                                span { class: "info-label", "expires" }
                                span { class: "info-value", "{exp}" }
                            }
                        }
                        if !wanted.is_empty() {
                            div { class: "info-row",
                                span { class: "info-label", "wanted" }
                                div { class: "cap-row",
                                    for m in wanted.iter() {
                                        span { class: "badge badge--dim", "{m}" }
                                    }
                                }
                            }
                        }
                        if !on_disk.is_empty() {
                            div { class: "info-row",
                                span { class: "info-label", "on disk" }
                                div { class: "cap-row",
                                    for m in on_disk.iter() {
                                        span { class: "badge badge--dim", "{m}" }
                                    }
                                }
                            }
                        }
                        if !invite.is_empty() {
                            div { class: "info-row",
                                span { class: "info-label", "invite" }
                                div { class: "token-row",
                                    code { class: "token-value",
                                        "{&invite[..invite.len().min(52)]}…"
                                    }
                                    CopyButton { value: invite.clone() }
                                }
                            }
                        }
                        div { class: "info-hint",
                            "Topology details require joining this mesh — discovery only carries metadata."
                        }
                    }
                }

                if let Some(ref err) = *join_error.read() {
                    div { class: "notice notice--err", "{err}" }
                }
            }
            div {
                class: "row-actions",
                onclick: move |evt: Event<MouseData>| evt.stop_propagation(),
                if attached && !*confirm.read() {
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |_| confirm.set(true),
                        "Join"
                    }
                } else if attached && *confirm.read() {
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
                                    let validate = sdk_session::validate_invite_token(&t);
                                    let stop_result = if validate.is_ok() {
                                        Some(process::stop_daemon())
                                    } else {
                                        None
                                    };
                                    async move {
                                        if let Err(e) = validate {
                                            join_error.set(Some(e));
                                            joining.set(false);
                                            confirm.set(false);
                                            return;
                                        }
                                        if let Some(Err(e)) = stop_result {
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
                                if let Err(e) = sdk_session::validate_invite_token(&t) {
                                    join_error.set(Some(e));
                                    joining.set(false);
                                    return;
                                }
                                if let Err(e) = process::spawn_join(&t) {
                                    join_error.set(Some(e));
                                    joining.set(false);
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

// ── Section 3 components ────────────────────────────────────────────────────

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
                        id: "home-pub-toggle",
                        checked: *is_public.read(),
                        onchange: move |e| is_public.set(e.checked()),
                    }
                    label { r#for: "home-pub-toggle", class: "field-label", "public (discoverable)" }
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
                        if let Err(e) = process::spawn_serve(&m, n_opt.as_deref(), pub_flag) {
                            error.set(Some(e));
                            pending.set(false);
                        }
                    },
                    if *pending.read() { "starting…" } else { "Start mesh" }
                }
            }
        }
    }
}
