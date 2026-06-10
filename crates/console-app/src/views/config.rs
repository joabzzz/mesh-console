//! Configuration tab — owner-gated runtime configuration with undo/redo and
//! compare-and-swap apply, matching the web console's configuration page:
//! Defaults, Deployment, Wake Policy, Signing, Integrations, and TOML Review.
//!
//! All edits operate on the JSON config snapshot from
//! `/api/runtime/control/get-config`; Apply sends it back with the expected
//! revision and surfaces conflicts as a rebase instead of losing edits.

use crate::state::{ControlBootstrap, MeshStatus};
use console_core::api::{ApiClient, ApplyOutcome};
use console_core::vm::config_history::{get_path, with_path_set, ConfigHistory};
use console_core::vm::config_toml;
use dioxus::prelude::*;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq)]
enum ConfigTab {
    Defaults,
    Deployment,
    WakePolicy,
    Signing,
    Integrations,
    Toml,
}

const TABS: &[(ConfigTab, &str)] = &[
    (ConfigTab::Defaults, "Defaults"),
    (ConfigTab::Deployment, "Deployment"),
    (ConfigTab::WakePolicy, "Wake Policy"),
    (ConfigTab::Signing, "Signing"),
    (ConfigTab::Integrations, "Integrations"),
    (ConfigTab::Toml, "TOML Review"),
];

#[component]
pub fn ConfigView(status: Signal<MeshStatus>, connected: Signal<bool>) -> Element {
    let mut bootstrap = use_signal(|| Option::<ControlBootstrap>::None);
    let mut history = use_signal(|| Option::<ConfigHistory>::None);
    let mut tab = use_signal(|| ConfigTab::Defaults);
    let mut error = use_signal(|| Option::<String>::None);
    let mut notice = use_signal(|| Option::<String>::None);
    let mut applying = use_signal(|| false);
    let mut load_attempted = use_signal(|| false);

    // (Re)load bootstrap + snapshot whenever the daemon connection appears.
    use_effect(move || {
        let is_connected = *connected.read();
        if !is_connected {
            bootstrap.set(None);
            history.set(None);
            load_attempted.set(false);
            return;
        }
        if *load_attempted.read() {
            return;
        }
        load_attempted.set(true);
        spawn(async move {
            let api = ApiClient::current();
            match api.fetch_control_bootstrap().await {
                Ok(boot) => {
                    let endpoint = boot.endpoint.clone();
                    let enabled = boot.enabled;
                    bootstrap.set(Some(boot));
                    if enabled {
                        match api.control_get_config(endpoint.as_deref()).await {
                            Ok(snapshot) => {
                                history.set(Some(ConfigHistory::new(
                                    snapshot.revision,
                                    snapshot.config,
                                )));
                                error.set(None);
                            }
                            Err(e) => error.set(Some(e)),
                        }
                    }
                }
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let owner_status = status
        .read()
        .owner
        .clone()
        .map(|o| o.status)
        .unwrap_or_default();
    let conn = *connected.read();
    let boot = bootstrap.read().clone();
    let dirty = history.read().as_ref().map(ConfigHistory::is_dirty);
    let can_undo = history
        .read()
        .as_ref()
        .map(ConfigHistory::can_undo)
        .unwrap_or(false);
    let can_redo = history
        .read()
        .as_ref()
        .map(ConfigHistory::can_redo)
        .unwrap_or(false);
    let revision = history.read().as_ref().map(ConfigHistory::base_revision);

    let apply = use_callback(move |_| {
        spawn(async move {
            let Some(h) = history.read().clone() else {
                return;
            };
            let endpoint = bootstrap.read().as_ref().and_then(|b| b.endpoint.clone());
            applying.set(true);
            error.set(None);
            notice.set(None);
            let api = ApiClient::current();
            let outcome = api
                .control_apply_config(endpoint.as_deref(), h.base_revision(), h.current().clone())
                .await;
            match outcome {
                Ok(ApplyOutcome::Applied(resp)) => {
                    if let Some(h) = history.write().as_mut() {
                        h.mark_applied(resp.current_revision);
                    }
                    notice.set(Some(format!(
                        "Applied — revision {} ({})",
                        resp.current_revision, resp.apply_mode
                    )));
                }
                Ok(ApplyOutcome::RevisionConflict { current_revision }) => {
                    // Reload the server state and rebase the user's edits.
                    match api.control_get_config(endpoint.as_deref()).await {
                        Ok(snapshot) => {
                            if let Some(h) = history.write().as_mut() {
                                h.rebase(snapshot.revision, snapshot.config);
                            }
                            notice.set(Some(format!(
                                "The config changed elsewhere (now revision {}). Your edits were kept on top — review and apply again.",
                                current_revision.unwrap_or_default()
                            )));
                        }
                        Err(e) => error.set(Some(e)),
                    }
                }
                Err(e) => error.set(Some(e)),
            }
            applying.set(false);
        });
    });

    rsx! {
        div { class: "view view--wide fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "Configuration" }
                if let Some(rev) = revision {
                    span { class: "badge badge--dim", "revision {rev}" }
                }
                if dirty == Some(true) {
                    span { class: "badge badge--warn", "unsaved changes" }
                }
            }

            if !conn {
                div { class: "empty-state",
                    span { "Daemon offline" }
                    span { class: "empty-hint", "Start or join a mesh to edit its configuration." }
                }
            } else if let Some(ref boot) = boot {
                if !boot.enabled {
                    div { class: "notice notice--warn",
                        p { {boot.disabled_reason.clone().or_else(|| boot.message.clone()).unwrap_or_else(|| "Configuration control is not available on this node.".to_string())} }
                        if owner_status != "verified" {
                            p { class: "empty-hint",
                                "Config changes require a verified owner identity on this node."
                            }
                        }
                    }
                } else if history.read().is_none() {
                    div { class: "empty-state", span { "Loading configuration…" } }
                } else {
                    // ── Tab bar ───────────────────────────────────────────
                    div { class: "config-tabs",
                        for (t, label) in TABS.iter() {
                            button {
                                class: if *tab.read() == *t { "config-tab config-tab--active" } else { "config-tab" },
                                onclick: {
                                    let t = *t;
                                    move |_| tab.set(t)
                                },
                                "{label}"
                            }
                        }
                    }

                    if let Some(ref err) = *error.read() {
                        div { class: "notice notice--err", "{err}" }
                    }
                    if let Some(ref ok) = *notice.read() {
                        div { class: "notice notice--ok", "{ok}" }
                    }

                    match *tab.read() {
                        ConfigTab::Defaults => rsx! { DefaultsTab { history } },
                        ConfigTab::Deployment => rsx! { DeploymentTab { history } },
                        ConfigTab::WakePolicy => rsx! { WakePolicyTab {} },
                        ConfigTab::Signing => rsx! { SigningTab { history, status } },
                        ConfigTab::Integrations => rsx! { IntegrationsTab { history } },
                        ConfigTab::Toml => rsx! { TomlTab { history } },
                    }

                    // ── Apply bar ─────────────────────────────────────────
                    div { class: "config-apply-bar",
                        button {
                            class: "btn btn--ghost btn--sm",
                            disabled: !can_undo,
                            onclick: move |_| { if let Some(h) = history.write().as_mut() { h.undo(); } },
                            "Undo"
                        }
                        button {
                            class: "btn btn--ghost btn--sm",
                            disabled: !can_redo,
                            onclick: move |_| { if let Some(h) = history.write().as_mut() { h.redo(); } },
                            "Redo"
                        }
                        button {
                            class: "btn btn--ghost btn--sm",
                            disabled: dirty != Some(true),
                            onclick: move |_| { if let Some(h) = history.write().as_mut() { h.reset_to_baseline(); } },
                            "Discard"
                        }
                        button {
                            class: "btn btn--primary",
                            disabled: dirty != Some(true) || *applying.read(),
                            onclick: move |_| apply.call(()),
                            if *applying.read() { "Applying…" } else { "Apply" }
                        }
                    }
                }
            } else {
                div { class: "empty-state", span { "Checking configuration control…" } }
            }
        }
    }
}

// ── Field editing helpers ─────────────────────────────────────────────────────

fn edit(history: &mut Signal<Option<ConfigHistory>>, path: &str, value: Value) {
    let next = {
        let guard = history.read();
        let Some(h) = guard.as_ref() else { return };
        with_path_set(h.current(), path, value)
    };
    if let Some(h) = history.write().as_mut() {
        h.push(next);
    }
}

fn text_at(history: &Signal<Option<ConfigHistory>>, path: &str) -> String {
    history
        .read()
        .as_ref()
        .and_then(|h| get_path(h.current(), path).cloned())
        .map(|v| match v {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn bool_at(history: &Signal<Option<ConfigHistory>>, path: &str) -> bool {
    history
        .read()
        .as_ref()
        .and_then(|h| get_path(h.current(), path).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn parse_field(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Value::Number(n);
        }
    }
    Value::String(trimmed.to_string())
}

#[component]
fn ConfigField(
    history: Signal<Option<ConfigHistory>>,
    label: String,
    path: String,
    placeholder: String,
    hint: String,
) -> Element {
    let mut history = history;
    let value = text_at(&history, &path);
    rsx! {
        div { class: "field config-field",
            label { class: "field-label", "{label}" }
            input {
                class: "field-input",
                r#type: "text",
                placeholder: "{placeholder}",
                value: "{value}",
                onchange: move |e| edit(&mut history, &path, parse_field(&e.value())),
            }
            if !hint.is_empty() {
                span { class: "field-hint", "{hint}" }
            }
        }
    }
}

// ── Tabs ─────────────────────────────────────────────────────────────────────

#[component]
fn DefaultsTab(history: Signal<Option<ConfigHistory>>) -> Element {
    rsx! {
        section { class: "section",
            h2 { class: "section-title", "Inference defaults" }
            p { class: "empty-hint", "Apply to every model unless a model entry overrides them." }
            div { class: "config-grid",
                ConfigField { history, label: "context size", path: "defaults.model_fit.ctx_size", placeholder: "8192", hint: "tokens per slot" }
                ConfigField { history, label: "parallel slots", path: "defaults.throughput.parallel", placeholder: "auto", hint: "concurrent requests" }
                ConfigField { history, label: "tuning profile", path: "defaults.throughput.tuning_profile", placeholder: "balanced", hint: "latency | balanced | throughput" }
                ConfigField { history, label: "KV cache type (K)", path: "defaults.model_fit.cache_type_k", placeholder: "f16", hint: "f16 | q8_0 | q4_0" }
                ConfigField { history, label: "KV cache type (V)", path: "defaults.model_fit.cache_type_v", placeholder: "f16", hint: "" }
                ConfigField { history, label: "batch", path: "defaults.model_fit.batch", placeholder: "2048", hint: "" }
            }
        }
        section { class: "section",
            h2 { class: "section-title", "Speculative decoding" }
            div { class: "config-grid",
                ConfigField { history, label: "mode", path: "defaults.speculative.mode", placeholder: "off", hint: "off | draft | ngram" }
                ConfigField { history, label: "draft selection policy", path: "defaults.speculative.draft_selection_policy", placeholder: "auto", hint: "" }
                ConfigField { history, label: "acceptance threshold", path: "defaults.speculative.draft_acceptance_threshold", placeholder: "0.75", hint: "0–1" }
                ConfigField { history, label: "draft max tokens", path: "defaults.speculative.draft_max_tokens", placeholder: "16", hint: "" }
            }
        }
        section { class: "section",
            h2 { class: "section-title", "Request defaults" }
            div { class: "config-grid",
                ConfigField { history, label: "temperature", path: "defaults.request_defaults.temperature", placeholder: "model default", hint: "" }
                ConfigField { history, label: "top_p", path: "defaults.request_defaults.top_p", placeholder: "model default", hint: "" }
                ConfigField { history, label: "max tokens", path: "defaults.request_defaults.max_tokens", placeholder: "unlimited", hint: "" }
            }
        }
    }
}

#[component]
fn DeploymentTab(history: Signal<Option<ConfigHistory>>) -> Element {
    let history = history;
    let models: Vec<(String, String, String)> = history
        .read()
        .as_ref()
        .and_then(|h| {
            get_path(h.current(), "models")
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let field = |key: &str| {
                m.get(key)
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_else(|| "inherit".to_string())
            };
            (
                m.get("model")
                    .and_then(Value::as_str)
                    .unwrap_or("(unnamed)")
                    .to_string(),
                field("ctx_size"),
                field("gpu_id"),
            )
        })
        .collect();

    rsx! {
        section { class: "section",
            h2 { class: "section-title", "GPU placement" }
            div { class: "config-grid",
                ConfigField { history, label: "assignment", path: "gpu.assignment", placeholder: "auto", hint: "auto | pinned" }
                ConfigField { history, label: "parallel models per GPU", path: "gpu.parallel", placeholder: "1", hint: "" }
            }
        }
        section { class: "section",
            h2 { class: "section-title", "Model entries" }
            if models.is_empty() {
                div { class: "empty-state",
                    span { "No per-model overrides" }
                    span { class: "empty-hint", "Add [[models]] entries in the TOML Review tab to pin context size, GPU, or quantization per model." }
                }
            }
            // Per-entry edits happen on the TOML Review tab; these rows are
            // read-only summaries of the [[models]] entries.
            for (name, ctx, gpu) in models {
                div { class: "info-card config-model-entry",
                    div { class: "info-row",
                        span { class: "info-label", "model" }
                        code { "{name}" }
                        span { class: "row-meta", "ctx {ctx}" }
                        span { class: "row-meta", "gpu {gpu}" }
                    }
                }
            }
        }
    }
}

#[component]
fn WakePolicyTab() -> Element {
    rsx! {
        section { class: "section",
            h2 { class: "section-title", "Wake policy" }
            div { class: "empty-state",
                span { "Wake scheduling for reserve nodes" }
                span { class: "empty-hint",
                    "Wakeable-node providers register through plugins; tune their wake policy in the provider's plugin section on the TOML Review tab. Live reserve state is on the Reserves tab."
                }
            }
        }
    }
}

#[component]
fn SigningTab(history: Signal<Option<ConfigHistory>>, status: Signal<MeshStatus>) -> Element {
    let mut history = history;
    let owner = status.read().owner.clone();
    let attestation = status.read().release_attestation.clone();
    let signer_keys = text_at(&history, "mesh_requirements.release_signer_keys");
    let require = bool_at(&history, "mesh_requirements.require_release_attestation");
    let _ = signer_keys;

    rsx! {
        section { class: "section",
            h2 { class: "section-title", "This node's identity" }
            div { class: "info-card",
                if let Some(ref o) = owner {
                    div { class: "info-row",
                        span { class: "info-label", "owner" }
                        if o.verified {
                            span { class: "badge badge--green", "{o.status}" }
                        } else {
                            span { class: "badge badge--warn", "{o.status}" }
                        }
                        if let Some(ref id) = o.owner_id {
                            code { class: "token-value", "{&id[..id.len().min(16)]}…" }
                        }
                    }
                }
                if let Some(ref a) = attestation {
                    div { class: "info-row",
                        span { class: "info-label", "release attestation" }
                        if a.verified {
                            span { class: "badge badge--green", "{a.status}" }
                        } else {
                            span { class: "badge badge--dim", "{a.status}" }
                        }
                        if let Some(ref commit) = a.commit {
                            code { "{commit}" }
                        }
                    }
                }
            }
        }
        section { class: "section",
            h2 { class: "section-title", "Mesh admission requirements" }
            div { class: "config-grid",
                ConfigField { history, label: "min node version", path: "mesh_requirements.min_node_version", placeholder: "any", hint: "" }
                ConfigField { history, label: "max node version", path: "mesh_requirements.max_node_version", placeholder: "any", hint: "" }
                ConfigField { history, label: "min protocol version", path: "mesh_requirements.min_protocol_version", placeholder: "any", hint: "" }
            }
            div { class: "field field--inline",
                input {
                    r#type: "checkbox",
                    id: "cfg-require-attestation",
                    checked: require,
                    onchange: move |e| edit(&mut history, "mesh_requirements.require_release_attestation", Value::Bool(e.checked())),
                }
                label { r#for: "cfg-require-attestation", class: "field-label",
                    "require release attestation from joining nodes"
                }
            }
        }
    }
}

#[component]
fn IntegrationsTab(history: Signal<Option<ConfigHistory>>) -> Element {
    let mut history = history;
    let plugins: Vec<(usize, String, bool)> = history
        .read()
        .as_ref()
        .and_then(|h| {
            get_path(h.current(), "plugin")
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                i,
                p.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                p.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            )
        })
        .collect();

    rsx! {
        section { class: "section",
            h2 { class: "section-title", "Plugins" }
            if plugins.is_empty() {
                div { class: "empty-state",
                    span { "No plugins configured" }
                    span { class: "empty-hint", "Add [[plugin]] entries (telemetry, blobstore, wake providers…) in the TOML Review tab." }
                }
            }
            div { class: "row-list",
                for (idx, name, enabled) in plugins {
                    div { class: "row-item",
                        div { class: "row-main",
                            span { class: "row-name", "{name}" }
                        }
                        div { class: "row-actions",
                            input {
                                r#type: "checkbox",
                                checked: enabled,
                                onchange: move |e| {
                                    let next = {
                                        let guard = history.read();
                                        let Some(h) = guard.as_ref() else { return };
                                        let mut root = h.current().clone();
                                        if let Some(list) = root.get_mut("plugin").and_then(Value::as_array_mut) {
                                            if let Some(entry) = list.get_mut(idx) {
                                                if let Some(obj) = entry.as_object_mut() {
                                                    obj.insert("enabled".to_string(), Value::Bool(e.checked()));
                                                }
                                            }
                                        }
                                        root
                                    };
                                    if let Some(h) = history.write().as_mut() {
                                        h.push(next);
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TomlTab(history: Signal<Option<ConfigHistory>>) -> Element {
    let mut history = history;
    let mut draft = use_signal(String::new);
    let mut toml_error = use_signal(|| Option::<String>::None);
    let mut staged = use_signal(|| false);

    // Refresh the editor whenever the underlying config changes (undo/apply).
    use_effect(move || {
        let rendered = history
            .read()
            .as_ref()
            .map(|h| config_toml::json_to_toml(h.current()).unwrap_or_default())
            .unwrap_or_default();
        draft.set(rendered);
        toml_error.set(None);
        staged.set(false);
    });

    rsx! {
        section { class: "section",
            h2 { class: "section-title", "TOML Review" }
            p { class: "empty-hint",
                "The full config as mesh-llm reads it (validated by the same parser the daemon uses). Stage your edits here, then Apply below."
            }
            textarea {
                class: "field-input config-toml-editor",
                spellcheck: false,
                value: "{draft.read()}",
                oninput: move |e| {
                    draft.set(e.value());
                    staged.set(false);
                },
            }
            if let Some(ref err) = *toml_error.read() {
                div { class: "notice notice--err", "{err}" }
            }
            if *staged.read() {
                div { class: "notice notice--ok", "Staged — use Apply below to save." }
            }
            div { class: "create-actions",
                button {
                    class: "btn btn--primary btn--sm",
                    onclick: move |_| {
                        match config_toml::toml_to_json(&draft.read()) {
                            Ok(value) => {
                                if let Some(h) = history.write().as_mut() {
                                    h.push(value);
                                }
                                toml_error.set(None);
                                staged.set(true);
                            }
                            Err(e) => toml_error.set(Some(e)),
                        }
                    },
                    "Validate & stage"
                }
            }
        }
    }
}
