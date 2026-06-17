//! Shared "join a mesh" affordance: paste an invite token *or* upload a photo
//! of the invite QR. Used both in first-run onboarding (`Step::Join`) and from
//! the always-available "Join" button in the header, so joining a private mesh
//! is one action away at any time.

use crate::components::node_status::NodePhaseBanner;
use crate::sdk_session;
use console_core::node_manager::{NodeManagerHandle, NodePhase, NodeSpec};
use dioxus::prelude::*;

/// The token field + QR-image upload + join button. Pulls the node manager and
/// phase from context (provided app-wide), so it is self-contained: drop it
/// anywhere a user should be able to join.
#[component]
pub fn JoinMeshForm() -> Element {
    let node = use_context::<NodeManagerHandle>();
    let node_phase = use_context::<Signal<NodePhase>>();

    let mut join_token = use_signal(String::new);
    let mut join_error = use_signal(|| Option::<String>::None);
    let decoding = use_signal(|| false);

    // Pick an image file, decode the QR off the main thread, and drop the
    // recovered token into the field (or surface a readable error).
    let on_upload = move |_| {
        let mut join_token = join_token;
        let mut join_error = join_error;
        let mut decoding = decoding;
        spawn(async move {
            let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("QR image", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                .pick_file()
                .await
            else {
                return;
            };
            decoding.set(true);
            join_error.set(None);
            let bytes = file.read().await;
            match console_core::qr_decode::decode_invite_qr(&bytes) {
                Ok(token) => join_token.set(token),
                Err(e) => join_error.set(Some(e)),
            }
            decoding.set(false);
        });
    };

    let is_decoding = *decoding.read();

    rsx! {
        div { class: "field",
            label { class: "field-label", "invite token" }
            input {
                class: "field-input",
                r#type: "text",
                placeholder: "mesh-…",
                value: "{join_token}",
                oninput: move |e| join_token.set(e.value()),
            }
        }
        div { class: "join-actions",
            button {
                class: "btn--ghost btn--sm",
                disabled: is_decoding,
                onclick: on_upload,
                if is_decoding { "Reading image…" } else { "Upload QR image" }
            }
            span { class: "join-actions-hint", "or scan it from a saved photo" }
        }
        if let Some(ref err) = *join_error.read() {
            div { class: "notice notice--err", "{err}" }
        }
        NodePhaseBanner {
            phase: node_phase,
            retry_spec: {
                let t = join_token.read().trim().to_string();
                (!t.is_empty()).then(|| NodeSpec::join(t))
            }
        }
        button {
            class: "btn onboarding-cta",
            disabled: join_token.read().trim().is_empty(),
            onclick: {
                let node = node.clone();
                move |_| {
                    let raw = join_token.read().trim().to_string();
                    join_error.set(None);
                    match sdk_session::validate_invite_token(&raw) {
                        Ok(token) => node.start(NodeSpec::join(token)),
                        Err(e) => join_error.set(Some(e)),
                    }
                }
            },
            "Join network"
        }
    }
}

/// Modal wrapper around [`JoinMeshForm`] for the always-available header entry.
///
/// Rendered at the app root (not inside the top nav) so its fixed-position
/// backdrop covers the viewport: under the `mesh` appearance the nav bar has a
/// `backdrop-filter`, which establishes a containing block and would otherwise
/// trap this modal into a popdown beneath the header.
#[component]
pub fn JoinMeshModal(on_close: EventHandler<()>) -> Element {
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal-card",
                onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h3 { class: "modal-title", "Join a mesh" }
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |_| on_close.call(()),
                        "close"
                    }
                }
                p { class: "modal-hint",
                    "Paste an invite token from the mesh's owner, or upload a photo of their invite QR. This machine joins and contributes its hardware."
                }
                JoinMeshForm {}
            }
        }
    }
}
