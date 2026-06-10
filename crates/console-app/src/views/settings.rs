//! Settings view: identity, node defaults, and on-disk data locations.

use crate::sdk_session;
use crate::state::MeshStatus;
use crate::views::dashboard::CopyButton;
use console_core::node_manager::{DEFAULT_API_PORT, DEFAULT_CONSOLE_PORT};
use console_core::settings;
use dioxus::prelude::*;

#[component]
pub fn SettingsView(status: Signal<MeshStatus>) -> Element {
    let owner = use_signal(|| sdk_session::owner_id().unwrap_or_else(|e| format!("error: {e}")));
    let saved = use_signal(settings::load);

    let st = status.read();
    let version = st.version.clone();
    let api_port = if st.api_port == 0 {
        DEFAULT_API_PORT
    } else {
        st.api_port
    };
    drop(st);

    let last = saved.read().last_node.clone();
    let config_dir = dirs::config_dir()
        .map(|d| d.join("mesh-console").display().to_string())
        .unwrap_or_default();
    let data_dir = dirs::data_local_dir()
        .map(|d| d.join("mesh-console").display().to_string())
        .unwrap_or_default();
    let cache_dir = dirs::cache_dir()
        .map(|d| d.join("mesh-llm").display().to_string())
        .unwrap_or_default();

    rsx! {
        div { class: "view fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "Settings" }
            }

            section { class: "section",
                h2 { class: "section-title", "Identity" }
                div { class: "info-card",
                    div { class: "info-row",
                        span { class: "info-label", "owner id" }
                        div { class: "token-row",
                            code { class: "token-value", "{owner.read()}" }
                            CopyButton { value: owner.read().clone() }
                        }
                    }
                    div { class: "info-hint",
                        "Generated on first launch and reused forever — peers see this as your stable identity. Both the chat client and the embedded node sign with it."
                    }
                }
            }

            section { class: "section",
                h2 { class: "section-title", "Node" }
                div { class: "info-card",
                    if let Some(ref cfg) = last {
                        div { class: "info-row",
                            span { class: "info-label", "last setup" }
                            span { class: "info-value",
                                "{cfg.mode}"
                                if !cfg.models.is_empty() {
                                    span { class: "row-meta", " · {cfg.models.join(\", \")}" }
                                }
                                if let Some(ref n) = cfg.mesh_name {
                                    span { class: "row-meta", " · mesh {n}" }
                                }
                            }
                        }
                    }
                    div { class: "info-row",
                        span { class: "info-label", "ports" }
                        span { class: "info-value",
                            "API :{api_port} · console :{DEFAULT_CONSOLE_PORT}"
                        }
                    }
                    if !version.is_empty() {
                        div { class: "info-row",
                            span { class: "info-label", "mesh-llm" }
                            span { class: "info-value", "v{version} (embedded)" }
                        }
                    }
                }
            }

            section { class: "section",
                h2 { class: "section-title", "Data on this machine" }
                div { class: "info-card",
                    div { class: "info-row",
                        span { class: "info-label", "identity & keys" }
                        code { class: "info-value", "{config_dir}" }
                    }
                    div { class: "info-row",
                        span { class: "info-label", "chats & settings" }
                        code { class: "info-value", "{data_dir}" }
                    }
                    div { class: "info-row",
                        span { class: "info-label", "models & runtime" }
                        code { class: "info-value", "{cache_dir}" }
                    }
                }
            }
        }
    }
}
