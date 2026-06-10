use crate::chat_persist::{
    self, AttachmentMeta, ChatArchive, ChatLine, ChatRole, ChatSessionRecord,
};
use crate::sdk_session;
use crate::state::{
    MeshStatus, ResponsesContent, ResponsesContentBlock, ResponsesInputMessage, ResponsesRequest,
    StreamOptions,
};
use console_core::api::{ApiClient, AttachmentFile, ChatStreamItem};
use dioxus::prelude::document;
use dioxus::prelude::*;

/// Stable-ish client id for routing affinity (mirrors the web console's
/// persisted client id).
fn chat_client_id() -> String {
    sdk_session::owner_id()
        .map(|id| format!("console-{}", &id[..id.len().min(12)]))
        .unwrap_or_else(|_| "console".to_string())
}

fn new_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("req-{ms}")
}

fn warm_model_names(status: &MeshStatus) -> Vec<String> {
    status
        .mesh_models
        .iter()
        .filter(|m| m.status == "warm")
        .map(|m| m.name.clone())
        .collect()
}

fn chat_endpoint_available(status: &MeshStatus) -> bool {
    let warm = !warm_model_names(status).is_empty();
    status.llama_ready || (status.is_client && warm)
}

fn default_model_selection(status: &MeshStatus) -> String {
    let warm = warm_model_names(status);
    if warm.len() > 1 {
        "auto".to_string()
    } else if warm.len() == 1 {
        warm[0].clone()
    } else if status.llama_ready && !status.model_name.is_empty() {
        status.model_name.clone()
    } else {
        "auto".to_string()
    }
}

fn model_option_list(st: &MeshStatus) -> Vec<String> {
    let warm = warm_model_names(st);
    let mut opts = Vec::new();
    if !warm.is_empty() {
        if warm.len() > 1 {
            opts.push("auto".to_string());
        }
        opts.extend(warm);
    }
    if st.llama_ready && !st.model_name.is_empty() && !opts.contains(&st.model_name) {
        opts.push(st.model_name.clone());
    }
    if opts.is_empty() {
        opts.push("auto".to_string());
    }
    opts
}

fn persist_best_effort(arch: &ChatArchive) {
    let _ = chat_persist::save_archive(arch);
}

#[component]
pub fn ChatView(status: Signal<MeshStatus>, connected: Signal<bool>) -> Element {
    let mut archive = use_signal(chat_persist::load_archive);
    let mut draft = use_signal(String::new);
    let mut selected_model = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut last_error = use_signal(|| Option::<String>::None);
    let mut pending_files = use_signal(Vec::<AttachmentFile>::new);

    use_effect(move || {
        if selected_model.read().is_empty() {
            selected_model.set(default_model_selection(&status.read()));
        }
    });

    use_effect(move || {
        let snap = archive.read().clone();
        let lines = snap
            .sessions
            .iter()
            .find(|s| s.id == snap.active_id)
            .map(|s| s.lines.clone())
            .unwrap_or_default();
        let n = lines.len();
        let tail = lines.last().map(|l| l.content.len()).unwrap_or(0);
        let _ = (n, tail);
        spawn(async move {
            let _ = document::eval(
                "requestAnimationFrame(()=>{const el=document.getElementById(\"mesh-chat-scroll\");if(el)el.scrollTop=el.scrollHeight;});",
            )
            .await;
        });
    });

    let send_message = use_callback(move |_| {
        spawn(async move {
            let text = draft.read().trim().to_string();
            if text.is_empty() {
                return;
            }
            let model = {
                let st_send = status.read();
                let opts_send = model_option_list(&st_send);
                let sel_raw = selected_model.read().clone();
                if opts_send.contains(&sel_raw) && !sel_raw.is_empty() {
                    sel_raw
                } else {
                    opts_send
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "auto".to_string())
                }
            };

            let can = {
                let st = status.read();
                let conn = *connected.read();
                conn && chat_endpoint_available(&st)
            };
            if !can {
                return;
            }

            busy.set(true);
            last_error.set(None);
            draft.set(String::new());

            let mut input: Vec<ResponsesInputMessage> = {
                let a = archive.read();
                a.sessions
                    .iter()
                    .find(|s| s.id == a.active_id)
                    .map(|s| {
                        s.lines
                            .iter()
                            .filter(|ln| !ln.content.is_empty())
                            .map(|ln| ResponsesInputMessage {
                                role: match ln.role {
                                    ChatRole::User => "user".to_string(),
                                    ChatRole::Assistant => "assistant".to_string(),
                                },
                                content: ResponsesContent::Text(ln.content.clone()),
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };

            let request_id = new_request_id();
            let api = ApiClient::current();

            // Upload pending attachments first; reference their tokens from
            // the user message as typed content blocks.
            let files: Vec<AttachmentFile> = pending_files.read().clone();
            pending_files.set(Vec::new());
            let mut attach_meta: Vec<AttachmentMeta> = Vec::new();
            let mut blocks: Vec<ResponsesContentBlock> = Vec::new();
            for file in &files {
                match api.upload_attachment(&request_id, file).await {
                    Ok(token) => {
                        blocks.push(match file.mime_type.split('/').next() {
                            Some("image") => ResponsesContentBlock::Image { image_url: token },
                            Some("audio") => ResponsesContentBlock::Audio { audio_url: token },
                            _ => ResponsesContentBlock::File {
                                url: token,
                                mime_type: Some(file.mime_type.clone()),
                                file_name: Some(file.file_name.clone()),
                            },
                        });
                        attach_meta.push(AttachmentMeta {
                            file_name: file.file_name.clone(),
                            mime_type: file.mime_type.clone(),
                        });
                    }
                    Err(e) => {
                        last_error.set(Some(e));
                        busy.set(false);
                        return;
                    }
                }
            }
            let content = if blocks.is_empty() {
                ResponsesContent::Text(text.clone())
            } else {
                blocks.push(ResponsesContentBlock::Text { text: text.clone() });
                ResponsesContent::Blocks(blocks)
            };
            input.push(ResponsesInputMessage {
                role: "user".to_string(),
                content,
            });

            {
                let mut a = archive.write();
                if let Some(sess) = a.active_session_mut() {
                    let mut user_line = ChatLine::new(ChatRole::User, text.clone());
                    user_line.attachments = attach_meta;
                    sess.lines.push(user_line);
                    sess.lines.push(ChatLine::new(ChatRole::Assistant, ""));
                    a.touch_active_title_from_user_message(&text);
                }
            }

            let request = ResponsesRequest {
                model,
                client_id: chat_client_id(),
                request_id,
                input,
                stream: true,
                stream_options: Some(StreamOptions {
                    include_usage: true,
                }),
            };

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ChatStreamItem>();
            let stream_task = tokio::spawn(async move { api.stream_responses(&request, tx).await });

            while let Some(item) = rx.recv().await {
                let mut a = archive.write();
                if let Some(sess) = a.active_session_mut() {
                    if let Some(last) = sess.lines.last_mut() {
                        if last.role == ChatRole::Assistant {
                            match item {
                                ChatStreamItem::TextDelta(delta) => last.content.push_str(&delta),
                                ChatStreamItem::ReasoningDelta(delta) => last
                                    .reasoning
                                    .get_or_insert_with(String::new)
                                    .push_str(&delta),
                                ChatStreamItem::Completed(done) => {
                                    last.usage = Some(done.usage);
                                    last.timings = done.timings;
                                    last.served_by = done.served_by;
                                }
                            }
                        }
                    }
                    sess.updated_at = chat_persist::now_unix();
                }
            }
            let result = stream_task
                .await
                .unwrap_or_else(|e| Err(format!("chat task failed: {e}")));

            busy.set(false);
            if let Err(e) = result {
                last_error.set(Some(e));
                let mut a = archive.write();
                if let Some(sess) = a.active_session_mut() {
                    if let Some(last) = sess.lines.last_mut() {
                        if last.role == ChatRole::Assistant && last.content.is_empty() {
                            last.content = "(no output)".to_string();
                        }
                    }
                }
            }
            persist_best_effort(&archive.read());
        });
    });

    let st = status.read();
    let conn = *connected.read();
    let warm = warm_model_names(&st);
    let api_port = st.api_port;
    let can_chat = conn && chat_endpoint_available(&st);
    let role_label = if st.is_client {
        "client"
    } else if st.is_host {
        "host"
    } else {
        "peer"
    };
    let hint = if st.is_client && warm.is_empty() {
        "No warm models in the catalog yet, so the SDK has nothing routable to chat with. Wait for a host peer to warm a model, or load one locally."
    } else if !st.llama_ready && !st.is_client {
        "This node is a host but llama_ready is false — load a model on the Models tab and wait for inference to come up before sending."
    } else {
        ""
    };

    let model_options = model_option_list(&st);

    let sel = selected_model.read().clone();
    let select_value = if model_options.contains(&sel) {
        sel
    } else {
        model_options
            .first()
            .cloned()
            .unwrap_or_else(|| "auto".to_string())
    };

    let arch_for_ui = archive.read().clone();
    let sidebar_sessions = arch_for_ui.sort_sessions_for_sidebar();
    let active_id = arch_for_ui.active_id.clone();
    let lines_for_scroll = arch_for_ui
        .sessions
        .iter()
        .find(|s| s.id == arch_for_ui.active_id)
        .map(|s| s.lines.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "view view--wide chat-view chat-view--layout fade-in",
            section { class: "section chat-view-header",
                h2 { class: "section-title", "Chat" }
                p { class: "chat-lede chat-lede--compact",
                    "Streams through "
                    code { "/api/responses" }
                    " · "
                    code { "api :{api_port}" }
                    " · "
                    span { class: "badge badge--dim", "{role_label}" }
                    if st.llama_ready {
                        span { class: "badge badge--green", style: "margin-left: 6px;", "llama_ready" }
                    }
                    if !warm.is_empty() {
                        span { class: "badge badge--dim", style: "margin-left: 6px;",
                            "warm: {warm.join(\", \")}"
                        }
                    }
                }
            }

            if !conn {
                div { class: "notice notice--warn",
                    "Not connected to mesh-llm on port 3131. Start or join a mesh from Network first."
                }
            } else if !can_chat {
                div { class: "notice notice--warn",
                    if !hint.is_empty() {
                        p { "{hint}" }
                    }
                    p { class: "chat-hint-secondary",
                        "Status: llama_ready={st.llama_ready}, is_client={st.is_client}, api_port={api_port}, model_name={st.model_name}"
                    }
                }
            }

            div { class: "chat-layout",
                aside { class: "chat-sidebar",
                    button {
                        class: "btn btn--primary btn--sm chat-sidebar-new",
                        disabled: *busy.read(),
                        onclick: move |_| {
                            let mut a = archive.write();
                            let id = chat_persist::new_session_id();
                            a.sessions.insert(
                                0,
                                ChatSessionRecord {
                                    id: id.clone(),
                                    title: "New chat".to_string(),
                                    updated_at: chat_persist::now_unix(),
                                    lines: Vec::new(),
                                },
                            );
                            a.active_id = id;
                            last_error.set(None);
                            persist_best_effort(&a.clone());
                        },
                        "New chat"
                    }
                    div { class: "chat-sidebar-list",
                        for sess in sidebar_sessions.iter() {
                            ChatSessionRow {
                                sid: sess.id.clone(),
                                title: sess.title.clone(),
                                is_active: sess.id == active_id,
                                busy: *busy.read(),
                                archive,
                                last_error,
                            }
                        }
                    }
                }

                div { class: "chat-main",
                    section { class: "section chat-controls chat-controls--inline",
                        label { class: "field-label", "Model" }
                        select {
                            class: "field-input chat-model-select",
                            disabled: *busy.read() || !can_chat,
                            value: "{select_value}",
                            onchange: move |e| selected_model.set(e.value()),
                            for opt in model_options.iter() {
                                option { value: "{opt}", "{opt}" }
                            }
                        }
                    }

                    if let Some(ref err) = *last_error.read() {
                        div { class: "notice notice--err", "{err}" }
                    }

                    div {
                        id: "mesh-chat-scroll",
                        class: "chat-scroll",
                        if lines_for_scroll.is_empty() {
                            div { class: "chat-empty",
                                "Type a message and press "
                                kbd { class: "chat-kbd", "Enter" }
                                " to send ("
                                kbd { class: "chat-kbd", "Shift+Enter" }
                                " for a new line). Replies stream in as they arrive."
                            }
                        }
                        for (i, line) in lines_for_scroll.iter().enumerate() {
                            ChatLineRow {
                                index: i,
                                line: line.clone(),
                                line_count: lines_for_scroll.len(),
                                busy: *busy.read(),
                            }
                        }
                    }

                    div { class: "chat-composer",
                        textarea {
                            class: "field-input chat-input",
                            placeholder: "Message…",
                            disabled: *busy.read() || !can_chat,
                            rows: 1,
                            value: "{draft.read()}",
                            oninput: move |e| draft.set(e.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter && !evt.modifiers().contains(Modifiers::SHIFT) {
                                    evt.prevent_default();
                                    if !*busy.read() && can_chat && !draft.read().trim().is_empty() {
                                        send_message.call(());
                                    }
                                }
                            },
                        }
                        if !pending_files.read().is_empty() {
                            div { class: "chat-attach-row",
                                for (i, file) in pending_files.read().iter().enumerate() {
                                    span { class: "chip chip--dim chat-attach-chip",
                                        "{file.file_name}"
                                        button {
                                            class: "chat-attach-remove",
                                            r#type: "button",
                                            onclick: move |_| {
                                                let mut files = pending_files.write();
                                                if i < files.len() { files.remove(i); }
                                            },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "chat-composer-bar",
                            span { class: "chat-composer-hint",
                                kbd { class: "chat-kbd", "Enter" }
                                " send · "
                                kbd { class: "chat-kbd", "Shift+Enter" }
                                " newline"
                            }
                            div { class: "chat-composer-actions",
                                button {
                                    class: "btn btn--ghost btn--sm",
                                    title: "Attach images, PDFs, or audio",
                                    disabled: *busy.read() || !can_chat,
                                    onclick: move |_| {
                                        spawn(async move {
                                            let picked = rfd::AsyncFileDialog::new()
                                                .add_filter(
                                                    "Attachments",
                                                    &["png", "jpg", "jpeg", "gif", "webp", "pdf", "wav", "mp3", "m4a", "txt", "md"],
                                                )
                                                .pick_files()
                                                .await
                                                .unwrap_or_default();
                                            for handle in picked {
                                                let file_name = handle.file_name();
                                                let bytes = handle.read().await;
                                                pending_files.write().push(AttachmentFile {
                                                    mime_type: console_core::api::objects::mime_type_for(&file_name),
                                                    file_name,
                                                    bytes,
                                                });
                                            }
                                        });
                                    },
                                    "+ Attach"
                                }
                                button {
                                    class: "btn btn--ghost btn--sm",
                                    disabled: *busy.read() || lines_for_scroll.is_empty(),
                                    onclick: move |_| {
                                        let mut a = archive.write();
                                        if let Some(sess) = a.active_session_mut() {
                                            sess.lines.clear();
                                            sess.title = "New chat".to_string();
                                            sess.updated_at = chat_persist::now_unix();
                                        }
                                        last_error.set(None);
                                        persist_best_effort(&a.clone());
                                    },
                                    "Clear"
                                }
                                button {
                                    class: "btn btn--primary",
                                    disabled: *busy.read() || !can_chat || draft.read().trim().is_empty(),
                                    onclick: move |_| send_message.call(()),
                                    if *busy.read() { "Sending…" } else { "Send" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChatSessionRow(
    sid: String,
    title: String,
    is_active: bool,
    busy: bool,
    mut archive: Signal<ChatArchive>,
    mut last_error: Signal<Option<String>>,
) -> Element {
    let row_class = if is_active {
        "chat-session chat-session--active"
    } else {
        "chat-session"
    };
    let sid_select = sid.clone();
    let sid_delete = sid;
    rsx! {
        div {
            class: row_class,
            onclick: move |_| {
                archive.write().active_id = sid_select.clone();
                last_error.set(None);
            },
            span { class: "chat-session-title", "{title}" }
            button {
                class: "chat-session-del",
                r#type: "button",
                disabled: busy,
                onclick: move |e| {
                    e.stop_propagation();
                    let mut a = archive.write();
                    a.sessions.retain(|s| s.id != sid_delete);
                    if a.sessions.is_empty() {
                        *a = ChatArchive::fresh();
                    } else if a.active_id == sid_delete {
                        a.active_id = a.sessions[0].id.clone();
                    }
                    last_error.set(None);
                    persist_best_effort(&a.clone());
                },
                "×"
            }
        }
    }
}

#[component]
fn ChatLineRow(index: usize, line: ChatLine, line_count: usize, busy: bool) -> Element {
    let is_last = index + 1 == line_count;
    let thinking = busy && is_last && line.role == ChatRole::Assistant && line.content.is_empty();
    let row_class = match line.role {
        ChatRole::User => "chat-row chat-row--user",
        ChatRole::Assistant => "chat-row chat-row--assistant",
    };
    let bubble_class = match line.role {
        ChatRole::User => "chat-bubble chat-bubble--user",
        ChatRole::Assistant => {
            if busy && is_last {
                "chat-bubble chat-bubble--assistant chat-bubble--streaming"
            } else {
                "chat-bubble chat-bubble--assistant"
            }
        }
    };
    // Response metadata footer (assistant lines, once completed).
    let meta_label = line.usage.as_ref().map(|usage| {
        let mut parts = vec![format!(
            "{}↑ {}↓ tokens",
            usage.input_tokens, usage.output_tokens
        )];
        if let Some(t) = &line.timings {
            if let Some(ttft) = t.ttft_ms {
                parts.push(format!("ttft {ttft:.0} ms"));
            }
            if let Some(total) = t.total_time_ms {
                parts.push(format!("total {:.1} s", total / 1000.0));
            }
        }
        parts.join(" · ")
    });
    let served_by = line.served_by.clone().filter(|s| !s.is_empty());
    let reasoning = line.reasoning.clone().filter(|r| !r.trim().is_empty());

    rsx! {
        div {
            class: row_class,
            div {
                class: bubble_class,
                if !line.attachments.is_empty() {
                    div { class: "chat-attach-row",
                        for att in line.attachments.iter() {
                            span { class: "chip chip--dim", "📎 {att.file_name}" }
                        }
                    }
                }
                if let Some(ref r) = reasoning {
                    details { class: "chat-reasoning",
                        summary { class: "chat-reasoning-summary", "Reasoning" }
                        pre { class: "chat-reasoning-text", "{r}" }
                    }
                }
                if thinking {
                    div { class: "chat-thinking",
                        span { class: "chat-thinking-dot" }
                        span { class: "chat-thinking-dot" }
                        span { class: "chat-thinking-dot" }
                    }
                } else {
                    pre { class: "chat-bubble-text", "{line.content}" }
                }
                if meta_label.is_some() || served_by.is_some() {
                    div { class: "chat-meta-row",
                        if let Some(ref meta) = meta_label {
                            span { class: "chat-meta", "{meta}" }
                        }
                        if let Some(ref peer) = served_by {
                            span { class: "badge badge--dim chat-meta-served", "via {peer}" }
                        }
                    }
                }
            }
        }
    }
}
