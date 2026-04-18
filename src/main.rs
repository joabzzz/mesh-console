mod api;
mod chat_persist;
mod components;
mod process;
mod sdk_session;
mod state;
mod status_display;
mod views;

use dioxus::prelude::*;
use state::{MeshStatus, RuntimeProcess};
use std::time::Duration;
use views::{ChatView, DashboardView, HomeView, ModelsView};

const MAIN_CSS: Asset = asset!("/assets/main.css");

const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(15);
const PROCESSES_REFRESH: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq)]
enum Nav {
    Home,
    Network,
    Chat,
    Models,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut status = use_signal(MeshStatus::default);
    let mut runtime_processes = use_signal(Vec::<RuntimeProcess>::new);
    let mut connected = use_signal(|| false);
    let mut nav = use_signal(|| Nav::Home);

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

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        div { class: "app app--topnav",
            header { class: "topnav",
                div { class: "topnav-left",
                    span { class: "topnav-brand",
                        span { class: "topnav-brand-icon", "⬡" }
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
                    }
                }
                div { class: "topnav-right",
                    button {
                        class: if *nav.read() == Nav::Models { "topnav-icon-btn topnav-icon-btn--active" } else { "topnav-icon-btn" },
                        onclick: move |_| nav.set(Nav::Models),
                        title: "Models",
                        "⊞"
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
                }
            }
        }
    }
}
