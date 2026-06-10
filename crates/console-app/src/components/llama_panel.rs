//! Local inference runtime panel: polls `/api/runtime/llama` while mounted
//! and shows slot occupancy + headline metrics — the data the web console's
//! NodeDrawer renders for the local node.

use crate::api;
use crate::state::LlamaRuntimePayload;
use dioxus::prelude::*;
use std::time::Duration;

const REFRESH: Duration = Duration::from_secs(5);

#[component]
pub fn LlamaRuntimePanel() -> Element {
    let mut payload = use_signal(|| Option::<LlamaRuntimePayload>::None);

    use_future(move || async move {
        loop {
            match api::fetch_runtime_llama().await {
                Ok(p) => payload.set(Some(p)),
                Err(_) => payload.set(None),
            }
            tokio::time::sleep(REFRESH).await;
        }
    });

    let Some(p) = payload.read().clone() else {
        return rsx! {};
    };
    if p.metrics.status != "ready" && p.slots.status != "ready" {
        return rsx! {};
    }

    let (slots_busy, slots_total) = p
        .items
        .as_ref()
        .map(|i| (i.slots_busy, i.slots_total))
        .unwrap_or_else(|| {
            let total = p.slots.slots.len() as u64;
            let busy = p.slots.slots.iter().filter(|s| s.is_processing).count() as u64;
            (busy, total)
        });

    // Surface the metrics the web console highlights, when present.
    let metric = |name: &str| {
        p.metrics
            .samples
            .iter()
            .find(|s| s.name.ends_with(name))
            .map(|s| s.value)
    };
    let prompt_tokens = metric("prompt_tokens_total");
    let predicted_tokens = metric("tokens_predicted_total");
    let kv_usage = metric("kv_cache_usage_ratio");

    rsx! {
        section { class: "section",
            h2 { class: "section-title", "Inference runtime" }
            div { class: "stat-grid stat-grid--compact",
                div { class: "stat-cell",
                    span { class: "stat-label", "slots" }
                    span { class: "stat-value", "{slots_busy} / {slots_total}" }
                }
                if let Some(v) = prompt_tokens {
                    div { class: "stat-cell",
                        span { class: "stat-label", "prompt tokens" }
                        span { class: "stat-value", "{v:.0}" }
                    }
                }
                if let Some(v) = predicted_tokens {
                    div { class: "stat-cell",
                        span { class: "stat-label", "generated tokens" }
                        span { class: "stat-value", "{v:.0}" }
                    }
                }
                if let Some(v) = kv_usage {
                    div { class: "stat-cell",
                        span { class: "stat-label", "kv cache" }
                        span { class: "stat-value", "{v * 100.0:.0}%" }
                    }
                }
            }
            if !p.slots.slots.is_empty() {
                div { class: "slot-row",
                    for slot in p.slots.slots.iter() {
                        span {
                            class: if slot.is_processing { "slot-dot slot-dot--busy" } else { "slot-dot" },
                            title: format!(
                                "slot {}{}",
                                slot.index.unwrap_or_default(),
                                if slot.is_processing { " — processing" } else { " — idle" }
                            ),
                        }
                    }
                }
            }
        }
    }
}
