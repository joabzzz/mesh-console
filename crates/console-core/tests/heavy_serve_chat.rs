//! Heavy end-to-end: serve-mode embedded node + real model + one streamed
//! chat round-trip through `/api/responses`. Downloads the native runtime
//! (~hundreds of MB, cached in ~/.cache/mesh-llm) and a small GGUF model on
//! first run, so it is double-gated:
//!
//! `MESH_CONSOLE_HEAVY_TESTS=1 cargo test -p console-core --test heavy_serve_chat -- --ignored`

#![cfg(feature = "embedded")]

use std::time::{Duration, Instant};

use console_core::api::{ApiClient, ChatStreamItem};
use console_core::node_manager::{NodeManager, NodePhase, NodeSpec};
use console_types::{ResponsesContent, ResponsesInputMessage, ResponsesRequest, StreamOptions};

const MODEL: &str = "unsloth/Qwen3-0.6B-GGUF:Q4_K_M";
const WARM_TIMEOUT: Duration = Duration::from_secs(900);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "downloads native runtime + GGUF model; set MESH_CONSOLE_HEAVY_TESTS=1"]
async fn serve_mode_chat_round_trip() {
    if std::env::var("MESH_CONSOLE_HEAVY_TESTS").as_deref() != Ok("1") {
        eprintln!("skipping: MESH_CONSOLE_HEAVY_TESTS != 1");
        return;
    }

    let manager = NodeManager::start();
    let handle = manager.handle();
    let mut rx = handle.subscribe();
    handle.start_alt_ports(NodeSpec::host(vec![MODEL.to_string()], None, false));

    // Phase: install (maybe) → starting → running.
    let deadline = Instant::now() + WARM_TIMEOUT;
    let console_url = loop {
        let phase = rx.borrow_and_update().clone();
        match phase {
            NodePhase::Running { console_url, .. } => break console_url,
            NodePhase::Failed { message, .. } => panic!("node failed: {message}"),
            _ => {}
        }
        assert!(Instant::now() < deadline, "node start timed out");
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    // Wait until the model is actually serving (GGUF download + load).
    let client = ApiClient::new(&console_url);
    loop {
        let status = client.fetch_status().await.expect("status");
        if status.serving_models.iter().any(|m| !m.is_empty()) && status.llama_ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "model never became warm; last status: serving={:?} ready={}",
            status.serving_models,
            status.llama_ready
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // One streamed chat round-trip with usage accounting.
    let request = ResponsesRequest {
        model: "auto".into(),
        client_id: "heavy-test".into(),
        request_id: format!("req-{}", std::process::id()),
        input: vec![ResponsesInputMessage {
            role: "user".into(),
            content: ResponsesContent::Text("Reply with exactly the word: pong".into()),
        }],
        stream: true,
        stream_options: Some(StreamOptions {
            include_usage: true,
        }),
    };
    let (tx, mut rx_items) = tokio::sync::mpsc::unbounded_channel();
    client
        .stream_responses(&request, tx)
        .await
        .expect("responses stream completes");

    let mut text = String::new();
    let mut completed = None;
    while let Ok(item) = rx_items.try_recv() {
        match item {
            ChatStreamItem::TextDelta(d) => text.push_str(&d),
            ChatStreamItem::Completed(c) => completed = Some(c),
            ChatStreamItem::ReasoningDelta(_) => {}
        }
    }
    assert!(!text.is_empty(), "model produced no text");
    let completed = completed.expect("completed event with usage");
    assert!(completed.usage.output_tokens > 0, "usage accounted");

    handle.stop();
    manager.shutdown_blocking(Duration::from_secs(15));
}
