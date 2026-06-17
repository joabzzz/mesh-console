mod components;
mod views;

// Re-export the non-UI layers under their historical module names so views
// keep using `crate::api`, `crate::state`, etc.
pub(crate) use console_core::{api, chat_persist, sdk_session, state, status_display};

use components::icons::{IconDial, IconGrid, IconSliders};
use components::invite_menu::InviteMenu;
use console_core::node_manager::{NodeManager, NodeManagerHandle, NodePhase};
use dioxus::prelude::*;
use state::{MeshStatus, RuntimeProcess};
use std::time::Duration;
use views::{
    ChatView, ConfigView, DashboardView, HomeView, ModelsView, OnboardingView, ReservesView,
    SettingsView,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");

const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(15);
const PROCESSES_REFRESH: Duration = Duration::from_secs(5);
const NODE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, PartialEq)]
enum Nav {
    Home,
    Network,
    Chat,
    Models,
    Reserves,
    Config,
    Settings,
}

fn main() {
    // The embedded mesh node lives on its own thread, outliving the UI event
    // loop so we can stop it cleanly after the window closes.
    let manager = NodeManager::start();
    let handle = manager.handle();
    dioxus::LaunchBuilder::new()
        .with_context(handle)
        .launch(App);
    manager.shutdown_blocking(NODE_SHUTDOWN_TIMEOUT);
}

#[component]
fn App() -> Element {
    let node = use_context::<NodeManagerHandle>();
    let mut status = use_signal(MeshStatus::default);
    let mut runtime_processes = use_signal(Vec::<RuntimeProcess>::new);
    let mut connected = use_signal(|| false);
    let mut nav = use_signal(|| Nav::Home);
    let mut node_phase = use_signal(NodePhase::default);
    use_context_provider(|| node_phase);
    // First run: no node has ever been started from this app.
    let mut show_onboarding = use_signal(|| console_core::settings::load().last_node.is_none());

    // Bridge the node manager's watch channel into a Dioxus signal, and point
    // the management-API client at the embedded node's console URL (it may be
    // on alternate ports).
    use_future({
        let node = node.clone();
        move || {
            let mut rx = node.subscribe();
            async move {
                loop {
                    let current = rx.borrow_and_update().clone();
                    if let NodePhase::Running { console_url, .. } = &current {
                        api::set_api_base(console_url.clone());
                    }
                    node_phase.set(current);
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    use_future(move || async move {
        // Periodic runtime/processes refresher. The SSE stream only carries
        // /api/status, so PIDs/ports for loaded models need a separate poll.
        spawn(async move {
            loop {
                match api::fetch_runtime_processes().await {
                    Ok(procs) => runtime_processes.set(procs),
                    Err(_) => runtime_processes.set(Vec::new()),
                }
                tokio::time::sleep(PROCESSES_REFRESH).await;
            }
        });

        // Outer loop: reconnect SSE with exponential backoff. Each connection
        // streams `MeshStatus` snapshots; we refresh `/api/models` when the
        // status-derived catalog key changes (mirroring the React hook).
        let mut backoff = RECONNECT_BACKOFF_INITIAL;
        let mut last_catalog_key = String::new();
        loop {
            // Best-effort initial fetch so the UI populates even if SSE is
            // slow or unavailable.
            if let Ok(mut initial) = api::fetch_status().await {
                if let Ok(models) = api::fetch_models().await {
                    last_catalog_key = api::model_catalog_key(&initial);
                    initial.mesh_models = models;
                }
                status.set(initial);
                connected.set(true);
            } else {
                connected.set(false);
            }

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<MeshStatus>();
            let stream_handle = tokio::spawn(async move {
                let _ = api::stream_status(tx).await;
            });

            while let Some(mut next) = rx.recv().await {
                backoff = RECONNECT_BACKOFF_INITIAL;
                connected.set(true);
                let key = api::model_catalog_key(&next);
                if key != last_catalog_key {
                    if let Ok(models) = api::fetch_models().await {
                        next.mesh_models = models;
                        last_catalog_key = key;
                    }
                } else {
                    let prev = status.read().mesh_models.clone();
                    next.mesh_models = prev;
                }
                status.set(next);
            }

            stream_handle.abort();
            connected.set(false);
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(RECONNECT_BACKOFF_MAX);
        }
    });

    if *show_onboarding.read() {
        return rsx! {
            document::Link { rel: "stylesheet", href: MAIN_CSS }
            div { class: "app app--topnav",
                OnboardingView {
                    on_done: move |_| {
                        show_onboarding.set(false);
                        nav.set(Nav::Network);
                    },
                }
            }
        };
    }

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        div { class: "app app--topnav",
            header { class: "topnav",
                div { class: "topnav-left",
                    span { class: "topnav-brand",
                        span { class: "topnav-brand-icon" }
                        span { class: "topnav-brand-name", "mesh" }
                        span { class: "topnav-brand-llm", "llm" }
                    }
                    div { class: "topnav-tabs",
                        button {
                            class: if *nav.read() == Nav::Home { "topnav-tab topnav-tab--active" } else { "topnav-tab" },
                            onclick: move |_| nav.set(Nav::Home),
                            "Home"
                        }
                        button {
                            class: if *nav.read() == Nav::Network { "topnav-tab topnav-tab--active" } else { "topnav-tab" },
                            onclick: move |_| nav.set(Nav::Network),
                            "Network"
                        }
                        button {
                            class: if *nav.read() == Nav::Chat { "topnav-tab topnav-tab--active" } else { "topnav-tab" },
                            onclick: move |_| nav.set(Nav::Chat),
                            "Chat"
                        }
                        button {
                            class: if *nav.read() == Nav::Reserves { "topnav-tab topnav-tab--active" } else { "topnav-tab" },
                            onclick: move |_| nav.set(Nav::Reserves),
                            "Reserves"
                        }
                    }
                }
                div { class: "topnav-right",
                    if let Some(latest) = status.read().latest_version.clone() {
                        if !status.read().version.is_empty() && latest != status.read().version {
                            span { class: "badge badge--warn", title: "A newer mesh-llm release is available",
                                "update {latest}"
                            }
                        }
                    }
                    InviteMenu { status }
                    button {
                        class: if *nav.read() == Nav::Config { "topnav-icon-btn topnav-icon-btn--active" } else { "topnav-icon-btn" },
                        onclick: move |_| nav.set(Nav::Config),
                        title: "Configuration",
                        IconSliders {}
                    }
                    button {
                        class: if *nav.read() == Nav::Models { "topnav-icon-btn topnav-icon-btn--active" } else { "topnav-icon-btn" },
                        onclick: move |_| nav.set(Nav::Models),
                        title: "Models",
                        IconGrid {}
                    }
                    button {
                        class: if *nav.read() == Nav::Settings { "topnav-icon-btn topnav-icon-btn--active" } else { "topnav-icon-btn" },
                        onclick: move |_| nav.set(Nav::Settings),
                        title: "Settings",
                        IconDial {}
                    }
                }
            }
            main { class: "topnav-content",
                match *nav.read() {
                    Nav::Home => rsx! {
                        HomeView {
                            status,
                            connected,
                            on_view_network: move |_| nav.set(Nav::Network),
                        }
                    },
                    Nav::Network => rsx! { DashboardView { status, connected } },
                    Nav::Chat => rsx! {
                        ChatView { status, connected }
                    },
                    Nav::Models => rsx! { ModelsView { status, runtime_processes } },
                    Nav::Reserves => rsx! { ReservesView { status, connected } },
                    Nav::Config => rsx! { ConfigView { status, connected } },
                    Nav::Settings => rsx! { SettingsView { status } },
                }
            }
        }
    }
}
