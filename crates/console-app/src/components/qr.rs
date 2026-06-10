//! Invite QR code modal + trigger button, shared by the header invite menu,
//! topology view, and onboarding success screen.

use crate::views::dashboard::CopyButton;
use dioxus::prelude::*;

pub fn invite_qr_svg(payload: &str) -> String {
    qrcode::QrCode::new(payload.as_bytes())
        .map(|code| {
            code.render::<qrcode::render::svg::Color<'_>>()
                .min_dimensions(220, 220)
                .quiet_zone(true)
                .dark_color(qrcode::render::svg::Color("#0c0d10"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_else(|_| String::from("<p>Failed to encode token.</p>"))
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
pub fn QrModal(token: String, on_close: EventHandler<()>) -> Element {
    let payload = format!("mesh-llm serve --join {token}");
    let svg = invite_qr_svg(&payload);

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
