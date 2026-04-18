use crate::api;
use crate::process;
use crate::state::{MeshModel, MeshStatus, RuntimeProcess};
use dioxus::prelude::*;

#[component]
pub fn ModelsView(
    status: Signal<MeshStatus>,
    runtime_processes: Signal<Vec<RuntimeProcess>>,
) -> Element {
    let mut action_error = use_signal(|| Option::<String>::None);
    let mut action_ok = use_signal(|| Option::<String>::None);

    let st = status.read();
    let mesh_models: Vec<MeshModel> = st.mesh_models.clone();
    let serving: Vec<String> = st.serving_models.clone();
    let available: Vec<String> = st.available_models.clone();
    let local_instances = st.local_instances.clone();
    drop(st);

    let processes = runtime_processes.read().clone();

    let do_load = move |model_name: String| {
        let name = model_name.clone();
        async move {
            action_error.set(None);
            action_ok.set(None);
            match api::load_model(&name).await {
                Ok(()) => action_ok.set(Some(format!("Loading {name}…"))),
                Err(e) => action_error.set(Some(e)),
            }
        }
    };

    let do_unload = move |model_name: String| {
        let name = model_name.clone();
        async move {
            action_error.set(None);
            action_ok.set(None);
            match api::unload_model(&name).await {
                Ok(()) => action_ok.set(Some(format!("Unloading {name}…"))),
                Err(e) => action_error.set(Some(e)),
            }
        }
    };

    rsx! {
        div { class: "view fade-in",
            header { class: "view-header",
                h1 { class: "view-title", "Models" }
            }

            if let Some(ref msg) = *action_ok.read() {
                div { class: "notice notice--ok", "{msg}" }
            }
            if let Some(ref err) = *action_error.read() {
                div { class: "notice notice--err", "{err}" }
            }

            if !processes.is_empty() {
                section { class: "section",
                    h2 { class: "section-title",
                        "Running Locally"
                        span { class: "count", " {processes.len()}" }
                    }
                    div { class: "row-list",
                        for proc in processes.iter() {
                            ProcessRow { proc: proc.clone() }
                        }
                    }
                }
            }

            if !mesh_models.is_empty() {
                section { class: "section",
                    h2 { class: "section-title",
                        "Mesh Models"
                        span { class: "count", " {mesh_models.len()}" }
                    }
                    div { class: "row-list",
                        for model in mesh_models.iter() {
                            ModelRow {
                                model: model.clone(),
                                serving: serving.contains(&model.name),
                                process: processes.iter().find(|p| p.name == model.name).cloned(),
                                on_load: {
                                    let name = model.name.clone();
                                    move |_| do_load(name.clone())
                                },
                                on_unload: {
                                    let name = model.name.clone();
                                    move |_| do_unload(name.clone())
                                },
                            }
                        }
                    }
                }
            }

            if !available.is_empty() {
                section { class: "section",
                    h2 { class: "section-title",
                        "Available Locally"
                        span { class: "count", " {available.len()}" }
                    }
                    div { class: "row-list",
                        for name in available.iter() {
                            if !mesh_models.iter().any(|m| &m.name == name) {
                                LocalModelRow {
                                    name: name.clone(),
                                    serving: serving.contains(name),
                                    on_load: {
                                        let n = name.clone();
                                        move |_| do_load(n.clone())
                                    },
                                    on_unload: {
                                        let n = name.clone();
                                        move |_| do_unload(n.clone())
                                    },
                                }
                            }
                        }
                    }
                }
            }

            if !local_instances.is_empty() {
                section { class: "section",
                    h2 { class: "section-title",
                        "Daemon Instances"
                        span { class: "count", " {local_instances.len()}" }
                    }
                    div { class: "row-list",
                        for inst in local_instances.iter() {
                            div { class: "row-item",
                                div { class: "row-main",
                                    span { class: "row-name",
                                        "pid {inst.pid}"
                                        if inst.is_self {
                                            span { class: "badge badge--green", style: "margin-left: 8px;", "this" }
                                        }
                                    }
                                    div { class: "cap-row",
                                        if let Some(port) = inst.api_port {
                                            span { class: "badge badge--dim", "api :{port}" }
                                        }
                                        if let Some(ref v) = inst.version {
                                            span { class: "badge badge--dim", "v{v}" }
                                        }
                                    }
                                }
                                div { class: "row-actions",
                                    button {
                                        class: "btn btn--ghost btn--sm",
                                        onclick: {
                                            let pid = inst.pid;
                                            move |_| {
                                                action_error.set(None);
                                                action_ok.set(None);
                                                match process::stop_pid(pid) {
                                                    Ok(()) => action_ok.set(Some(format!("Sent stop to pid {pid}"))),
                                                    Err(e) => action_error.set(Some(e)),
                                                }
                                            }
                                        },
                                        "stop"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if mesh_models.is_empty() && available.is_empty() && processes.is_empty() {
                div { class: "empty-state",
                    span { "No models found" }
                    span { class: "empty-hint", "Start mesh-llm and it will appear here." }
                }
            }
        }
    }
}

#[component]
fn ModelRow(
    model: MeshModel,
    serving: bool,
    process: Option<RuntimeProcess>,
    on_load: EventHandler<MouseEvent>,
    on_unload: EventHandler<MouseEvent>,
) -> Element {
    let display = if model.display_name.is_empty() {
        model.name.clone()
    } else {
        model.display_name.clone()
    };

    let meta = {
        let mut parts = Vec::new();
        if let Some(ref arch) = model.architecture {
            parts.push(arch.clone());
        }
        if let Some(ref q) = model.quantization {
            parts.push(q.clone());
        }
        if model.size_gb > 0.0 {
            parts.push(format!("{:.1} GB", model.size_gb));
        }
        if let Some(ctx) = model.context_length {
            parts.push(format!("{}k ctx", ctx / 1000));
        }
        parts.join(" · ")
    };

    let caps = {
        let mut c = Vec::new();
        if model.vision {
            c.push("vision");
        }
        if model.audio {
            c.push("audio");
        }
        if model.reasoning {
            c.push("reasoning");
        }
        if model.tool_use {
            c.push("tools");
        }
        if model.moe {
            c.push("moe");
        }
        c
    };

    rsx! {
        div { class: "row-item row-item--model",
            div { class: "row-main",
                span { class: "row-name", "{display}" }
                if !meta.is_empty() {
                    span { class: "row-meta", "{meta}" }
                }
                div { class: "cap-row",
                    for cap in caps.iter() {
                        span { class: "badge badge--dim", "{cap}" }
                    }
                    if serving {
                        span { class: "badge badge--green", "serving" }
                    } else {
                        span { class: "badge badge--muted", "{model.status}" }
                    }
                    if model.node_count > 0 {
                        span { class: "badge badge--dim", "{model.node_count} nodes" }
                    }
                    if let Some(ref p) = process {
                        span { class: "badge badge--dim", "{p.backend}" }
                        span { class: "badge badge--dim", "pid {p.pid}" }
                        span { class: "badge badge--dim", ":{p.port}" }
                    }
                }
            }
            div { class: "row-actions",
                if serving {
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |e| on_unload.call(e),
                        "unload"
                    }
                } else {
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |e| on_load.call(e),
                        "load"
                    }
                }
            }
        }
    }
}

#[component]
fn LocalModelRow(
    name: String,
    serving: bool,
    on_load: EventHandler<MouseEvent>,
    on_unload: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "row-item",
            div { class: "row-main",
                span { class: "row-name", "{name}" }
                if serving {
                    span { class: "badge badge--green", "serving" }
                }
            }
            div { class: "row-actions",
                if serving {
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |e| on_unload.call(e),
                        "unload"
                    }
                } else {
                    button {
                        class: "btn btn--ghost btn--sm",
                        onclick: move |e| on_load.call(e),
                        "load"
                    }
                }
            }
        }
    }
}

#[component]
fn ProcessRow(proc: RuntimeProcess) -> Element {
    rsx! {
        div { class: "row-item",
            div { class: "row-main",
                span { class: "row-name", "{proc.name}" }
                div { class: "cap-row",
                    span { class: "badge badge--dim", "{proc.backend}" }
                    span { class: "badge badge--dim", "{proc.status}" }
                    span { class: "badge badge--dim", "pid {proc.pid}" }
                    span { class: "badge badge--dim", ":{proc.port}" }
                }
            }
        }
    }
}
