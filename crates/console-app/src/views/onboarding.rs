//! First-run onboarding: a staged, full-window experience.
//!
//!   1. Welcome — what this is, what we detected about the machine,
//!      choose to create a network or join one.
//!   2. Create (pick a starter model + name) — or — Join (paste a token).
//!   3. Working — runtime download / node boot with live progress.
//!   4. Live — success, QR invite, into the console.
//!
//! Shown until the first node start succeeds (tracked via
//! `settings.last_node`) or the user skips.

use crate::components::node_status::NodePhaseBanner;
use crate::components::qr::invite_qr_svg;
use crate::sdk_session;
use crate::views::dashboard::CopyButton;
use console_core::node_manager::{NodeManagerHandle, NodePhase, NodeSpec};
use console_core::vm::invite;
use console_core::vm::onboarding::{
    detect_hardware, recommend_models, HardwareSummary, StarterModel,
};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Welcome,
    Create,
    Join,
}

#[component]
pub fn OnboardingView(on_done: EventHandler<()>) -> Element {
    let node = use_context::<NodeManagerHandle>();
    let node_phase = use_context::<Signal<NodePhase>>();

    let hardware = use_signal(|| Option::<HardwareSummary>::None);
    let mut step = use_signal(|| Step::Welcome);
    let mut selected_model = use_signal(String::new);
    let mut mesh_name = use_signal(String::new);
    let mut join_token = use_signal(String::new);
    let mut join_error = use_signal(|| Option::<String>::None);

    // Hardware probe touches Metal; keep it off the render thread.
    use_future({
        let mut hardware = hardware;
        move || async move {
            let detected = tokio::task::spawn_blocking(detect_hardware)
                .await
                .ok()
                .flatten();
            hardware.set(detected);
        }
    });

    let hw = hardware.read().clone();
    let vram_gb = hw.as_ref().map(|h| h.vram_gb).unwrap_or(0.0);
    let starters: Vec<StarterModel> = recommend_models(vram_gb);
    let default_model = starters.first().map(|m| m.model_ref.to_string());
    if selected_model.read().is_empty() {
        if let Some(m) = default_model {
            selected_model.set(m);
        }
    }

    let phase = node_phase.read().clone();
    let busy = phase.is_busy();

    // ── Stage 4: live ────────────────────────────────────────────────────
    if let NodePhase::Running {
        invite_token,
        api_port,
        ..
    } = &phase
    {
        let token = invite_token.clone().unwrap_or_default();
        let qr =
            (!token.is_empty()).then(|| invite_qr_svg(&format!("mesh-llm serve --join {token}")));
        let api_url = invite::api_direct_url(*api_port);
        return rsx! {
            div { class: "view onboarding fade-in",
                div { class: "onboarding-card onboarding-card--success",
                    StepHeader { label: "live".to_string(), progress: 100 }
                    h1 { class: "onboarding-title", "Your network is live." }
                    p { class: "onboarding-lede",
                        "This machine is now a host. Other machines join by scanning the code — phones, laptops, anything that can run mesh-llm."
                    }
                    if let Some(ref svg) = qr {
                        div { class: "qr-frame qr-frame--inline",
                            div { class: "qr-svg", dangerous_inner_html: "{svg}" }
                        }
                        if !token.is_empty() {
                            div { class: "token-row",
                                code { class: "token-value", "{&token[..token.len().min(44)]}…" }
                                CopyButton { value: token.clone() }
                            }
                        }
                    }
                    if let Some(ref url) = api_url {
                        p { class: "onboarding-hint",
                            "Any OpenAI-compatible app can use it at "
                            code { "{url}" }
                        }
                    }
                    button {
                        class: "btn onboarding-cta",
                        onclick: move |_| on_done.call(()),
                        "Open the console"
                    }
                }
            }
        };
    }

    // ── Stage 3: working (install / boot) ───────────────────────────────
    if busy {
        let (label, headline, sub) = match &phase {
            NodePhase::InstallingRuntime { .. } => (
                "step 2 of 3",
                "Downloading the inference engine…",
                "One-time download, cached for every future launch.",
            ),
            _ => (
                "step 3 of 3",
                "Starting your node…",
                "Fetching the model and warming it up. Bigger models take a few minutes on first run.",
            ),
        };
        return rsx! {
            div { class: "view onboarding fade-in",
                div { class: "onboarding-card",
                    StepHeader { label: label.to_string(), progress: 80 }
                    h1 { class: "onboarding-title", "{headline}" }
                    p { class: "onboarding-lede", "{sub}" }
                    NodePhaseBanner { phase: node_phase, retry_spec: None }
                }
            }
        };
    }

    // ── Stage 1 + 2 ──────────────────────────────────────────────────────
    let current = *step.read();
    rsx! {
        div { class: "view onboarding fade-in",
            div { class: "onboarding-card",
                match current {
                    Step::Welcome => rsx! {
                        StepHeader { label: "step 1 of 3".to_string(), progress: 16 }
                        h1 { class: "onboarding-title", "Your hardware. Your models. Your network." }
                        p { class: "onboarding-lede",
                            "mesh-console turns this machine into a private AI network: host open models locally, invite other machines, and use it all through one OpenAI-compatible endpoint. No cloud. No accounts."
                        }
                        if let Some(ref hw) = hw {
                            div { class: "onboarding-hw",
                                span { class: "badge badge--info", "{hw.gpu_name}" }
                                span { class: "badge badge--dim", "{hw.vram_gb:.0} GB memory" }
                                if hw.unified_memory {
                                    span { class: "badge badge--dim", "unified" }
                                }
                            }
                        }
                        div { class: "onboarding-paths",
                            button {
                                class: "onboarding-path",
                                onclick: move |_| step.set(Step::Create),
                                "Create my network"
                            }
                            button {
                                class: "onboarding-path",
                                onclick: move |_| step.set(Step::Join),
                                "Join an existing one"
                            }
                        }
                        button {
                            class: "btn--ghost btn--sm onboarding-skip",
                            onclick: move |_| on_done.call(()),
                            "Skip — take me to the console"
                        }
                    },
                    Step::Create => rsx! {
                        StepHeader { label: "step 2 of 3".to_string(), progress: 50 }
                        h1 { class: "onboarding-title", "Pick your first model." }
                        p { class: "onboarding-lede",
                            "Sized to this machine — you can add or swap models any time."
                        }
                        div { class: "onboarding-models",
                            for starter in starters.iter() {
                                {
                                    let model_ref = starter.model_ref.to_string();
                                    let active = *selected_model.read() == model_ref;
                                    let label = starter.label;
                                    let blurb = starter.blurb;
                                    let size = starter.size_gb;
                                    rsx! {
                                        button {
                                            class: if active { "onboarding-model onboarding-model--active" } else { "onboarding-model" },
                                            onclick: move |_| selected_model.set(model_ref.clone()),
                                            span { class: "onboarding-model-name", "{label}" }
                                            span { class: "onboarding-model-blurb", "{blurb}" }
                                            span { class: "onboarding-model-size", "{size:.1} GB" }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "field",
                            label { class: "field-label", "network name — optional" }
                            input {
                                class: "field-input",
                                r#type: "text",
                                placeholder: "e.g. den-mesh",
                                value: "{mesh_name}",
                                oninput: move |e| mesh_name.set(e.value()),
                            }
                        }
                        NodePhaseBanner {
                            phase: node_phase,
                            retry_spec: Some(NodeSpec::host(
                                vec![selected_model.read().clone()],
                                Some(mesh_name.read().trim().to_string()).filter(|n| !n.is_empty()),
                                false,
                            ))
                        }
                        button {
                            class: "btn onboarding-cta",
                            disabled: selected_model.read().is_empty(),
                            onclick: {
                                let node = node.clone();
                                move |_| {
                                    let model = selected_model.read().clone();
                                    let name = mesh_name.read().trim().to_string();
                                    node.start(NodeSpec::host(
                                        vec![model],
                                        (!name.is_empty()).then_some(name),
                                        false,
                                    ));
                                }
                            },
                            "Create my private network"
                        }
                        button {
                            class: "btn--ghost btn--sm onboarding-skip",
                            onclick: move |_| step.set(Step::Welcome),
                            "Back"
                        }
                    },
                    Step::Join => rsx! {
                        StepHeader { label: "step 2 of 3".to_string(), progress: 50 }
                        h1 { class: "onboarding-title", "Paste your invite." }
                        p { class: "onboarding-lede",
                            "Get a token from the network's owner — it's under Invite in their console. This machine will contribute its hardware and gain access to every model on the network."
                        }
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
                        button {
                            class: "btn--ghost btn--sm onboarding-skip",
                            onclick: move |_| step.set(Step::Welcome),
                            "Back"
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn StepHeader(label: String, progress: u8) -> Element {
    rsx! {
        div { class: "onboarding-step",
            span { "{label}" }
            div { class: "onboarding-step-bar",
                div { class: "onboarding-step-bar-fill", style: "width: {progress}%;" }
            }
        }
    }
}
