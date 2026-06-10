//! Header invite menu: copy invite commands (contribute-compute, client,
//! raw token), the OpenAI-compatible API URL, and the QR code — the same
//! affordances as the web console's AppHeader invite/API popovers.

use crate::components::qr::invite_qr_svg;
use crate::state::MeshStatus;
use crate::views::dashboard::CopyButton;
use console_core::vm::invite;
use dioxus::prelude::*;

#[component]
pub fn InviteMenu(status: Signal<MeshStatus>) -> Element {
    let mut open = use_signal(|| false);

    let st = status.read();
    let token = st.token.clone();
    let api_port = st.api_port;
    let primary_model = st
        .serving_models
        .iter()
        .chain(st.hosted_models.iter())
        .find(|m| !m.is_empty())
        .cloned();
    drop(st);

    let has_token = !token.is_empty();
    let with_model = invite::invite_with_model_command(&token, primary_model.as_deref());
    let as_client = invite::invite_client_command(&token);
    let api_url = invite::api_direct_url(api_port);
    let qr_svg = invite_qr_svg(&format!("mesh-llm serve --join {token}"));

    rsx! {
        button {
            class: "btn btn--ghost btn--sm topnav-invite",
            disabled: !has_token,
            title: if has_token { "Invite others to this mesh" } else { "Start or join a mesh first" },
            onclick: move |_| open.set(true),
            "Invite"
        }
        if *open.read() {
            div { class: "modal-backdrop", onclick: move |_| open.set(false),
                div {
                    class: "modal-card modal-card--wide",
                    onclick: move |e| e.stop_propagation(),
                    div { class: "modal-head",
                        h3 { class: "modal-title", "Invite to this mesh" }
                        button {
                            class: "btn btn--ghost btn--sm",
                            onclick: move |_| open.set(false),
                            "close"
                        }
                    }
                    p { class: "modal-hint",
                        "Invite with a model loaded to add compute, or invite as a client for API-only access."
                    }

                    div { class: "qr-frame qr-frame--inline",
                        div { class: "qr-svg", dangerous_inner_html: "{qr_svg}" }
                    }

                    if let Some(ref cmd) = with_model {
                        div { class: "invite-block",
                            div { class: "invite-block-title",
                                "Contribute compute "
                                span { class: "badge badge--green", "recommended" }
                            }
                            div { class: "token-row",
                                code { class: "token-value", "{cmd}" }
                                CopyButton { value: cmd.clone() }
                            }
                        }
                    }
                    if let Some(ref cmd) = as_client {
                        div { class: "invite-block",
                            div { class: "invite-block-title", "Join as client" }
                            div { class: "token-row",
                                code { class: "token-value", "{cmd}" }
                                CopyButton { value: cmd.clone() }
                            }
                        }
                    }
                    div { class: "invite-block",
                        div { class: "invite-block-title", "Raw invite token" }
                        div { class: "token-row",
                            code { class: "token-value", "{&token[..token.len().min(56)]}…" }
                            CopyButton { value: token.clone() }
                        }
                    }
                    if let Some(ref url) = api_url {
                        div { class: "invite-block",
                            div { class: "invite-block-title", "API access" }
                            p { class: "modal-hint",
                                "OpenAI-compatible endpoint — works with any app that speaks the OpenAI API."
                            }
                            div { class: "token-row",
                                code { class: "token-value", "{url}" }
                                CopyButton { value: url.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}
