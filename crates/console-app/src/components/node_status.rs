//! Banner that renders the embedded node's lifecycle phase: runtime download
//! progress, boot spinner, and failures (with a one-click "use alternate
//! ports" recovery for port conflicts). Rendered by Home and the Network
//! start view so the node's state is always visible where users act on it.

use console_core::node_manager::{FailureKind, NodeManagerHandle, NodePhase, NodeSpec};
use dioxus::prelude::*;

/// Pretty-print a byte count for the download progress line.
fn fmt_bytes(bytes: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

/// Renders nothing while the node is `Idle` or `Running` — those states are
/// represented by the surrounding view.
#[component]
pub fn NodePhaseBanner(
    phase: Signal<NodePhase>,
    /// The spec to retry with when the user picks "use alternate ports".
    /// `None` hides the retry button.
    retry_spec: Option<NodeSpec>,
) -> Element {
    let node = use_context::<NodeManagerHandle>();
    let current = phase.read().clone();

    match current {
        NodePhase::Idle | NodePhase::Running { .. } => rsx! {},
        NodePhase::InstallingRuntime {
            downloaded_bytes,
            total_bytes,
        } => {
            let (pct, detail) = match total_bytes {
                Some(total) if total > 0 => (
                    Some((downloaded_bytes as f64 / total as f64 * 100.0).min(100.0)),
                    format!("{} / {}", fmt_bytes(downloaded_bytes), fmt_bytes(total)),
                ),
                _ => (None, fmt_bytes(downloaded_bytes)),
            };
            rsx! {
                div { class: "notice notice--info node-phase",
                    div { class: "node-phase-title", "Downloading inference runtime…" }
                    div { class: "node-phase-detail", "{detail}" }
                    if let Some(pct) = pct {
                        div { class: "progress-track",
                            div { class: "progress-fill", style: "width: {pct:.0}%;" }
                        }
                    }
                    div { class: "node-phase-hint",
                        "One-time download — the runtime is cached for future launches."
                    }
                }
            }
        }
        NodePhase::Starting => rsx! {
            div { class: "notice notice--info node-phase",
                div { class: "node-phase-title", "Starting mesh node…" }
            }
        },
        NodePhase::Stopping => rsx! {
            div { class: "notice notice--info node-phase",
                div { class: "node-phase-title", "Stopping mesh node…" }
            }
        },
        NodePhase::Failed { message, kind } => {
            let retryable = matches!(
                kind,
                FailureKind::ExternalDaemon | FailureKind::PortConflict
            ) && retry_spec.is_some();
            rsx! {
                div { class: "notice notice--err node-phase",
                    div { class: "node-phase-title", "{message}" }
                    if retryable {
                        button {
                            class: "btn btn--ghost btn--sm",
                            onclick: {
                                let node = node.clone();
                                let spec = retry_spec.clone();
                                move |_| {
                                    if let Some(spec) = spec.clone() {
                                        node.start_alt_ports(spec);
                                    }
                                }
                            },
                            "Start on alternate ports"
                        }
                    }
                }
            }
        }
    }
}
