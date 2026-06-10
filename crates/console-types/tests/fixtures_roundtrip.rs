//! Drift alarm: every fixture (live-captured and synthetic) must parse into
//! our payload mirrors, and key fields must survive. When a mesh-llm bump
//! changes the wire format, re-capture with
//! `cargo test -p console-core --test capture_fixtures -- --ignored`
//! and fix whatever breaks here.

use console_types::*;

#[test]
fn live_client_status_parses() {
    let raw = include_str!("fixtures/status_client.json");
    let status: MeshStatus = serde_json::from_str(raw).expect("parse live status");
    assert!(!status.node_id.is_empty());
    assert!(status.is_client);
    assert_eq!(status.node_state, "client");
    let owner = status.owner.expect("owner block present");
    assert_eq!(owner.status, "verified");
    assert!(owner.verified);
    assert!(status.local_instances.iter().any(|i| i.is_self));
    assert!(
        status.release_attestation.is_some(),
        "attestation block present"
    );
}

#[test]
fn synthetic_full_status_parses_with_all_blocks() {
    let raw = include_str!("fixtures/status_full.json");
    let status: MeshStatus = serde_json::from_str(raw).expect("parse full status");

    assert_eq!(status.node_state, "serving");
    assert_eq!(status.latest_version.as_deref(), Some("0.69.0"));
    assert_eq!(status.publication_state, "public");
    assert_eq!(status.token, "mesh-invite-token-xyz");

    let runtime = status.runtime.expect("runtime block");
    assert_eq!(runtime.backend.as_deref(), Some("skippy"));
    assert_eq!(runtime.models.len(), 1);
    assert_eq!(runtime.stages.len(), 1);
    assert_eq!(runtime.stages[0].layer_end, 35);

    assert_eq!(status.peers.len(), 2);
    let host = &status.peers[0];
    assert_eq!(host.state, "serving");
    assert_eq!(host.latency_ms, Some(11));
    assert_eq!(host.latency_source.as_deref(), Some("direct"));
    assert_eq!(host.version.as_deref(), Some("0.68.0"));
    assert!(host.owner.as_ref().unwrap().verified);
    assert_eq!(host.gpus[0].mem_bandwidth_gbps, Some(400.0));

    let client = &status.peers[1];
    assert!(!client.hosted_models_known);
    assert_eq!(
        client.release_attestation.as_ref().unwrap().status,
        "invalid"
    );

    assert_eq!(status.wakeable_nodes.len(), 2);
    assert_eq!(status.wakeable_nodes[0].state, "sleeping");
    assert_eq!(status.wakeable_nodes[1].wake_eta_secs, Some(95));
    assert_eq!(status.wakeable_nodes[1].provider.as_deref(), Some("fly.io"));

    let attestation = status.release_attestation.unwrap();
    assert_eq!(attestation.status, "valid");
    assert!(attestation.verified);
}

#[test]
fn models_fixture_parses_with_capabilities() {
    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        mesh_models: Vec<MeshModel>,
    }
    let raw = include_str!("fixtures/models_full.json");
    let parsed: ModelsResponse = serde_json::from_str(raw).expect("parse models");
    assert_eq!(parsed.mesh_models.len(), 2);

    let warm = &parsed.mesh_models[0];
    assert_eq!(warm.status, "warm");
    assert_eq!(warm.context_length, Some(40960));
    assert!(warm.reasoning && warm.tool_use && !warm.vision);
    assert_eq!(warm.active_nodes.len(), 2);
    assert_eq!(warm.draft_model.as_deref(), Some("Qwen3-0.6B-Q4_K_M"));

    let cold = &parsed.mesh_models[1];
    assert_eq!(cold.status, "cold");
    assert!(cold.vision);
    assert_eq!(cold.node_count, 0);
}

#[test]
fn runtime_llama_fixture_parses() {
    let raw = include_str!("fixtures/runtime_llama.json");
    let payload: LlamaRuntimePayload = serde_json::from_str(raw).expect("parse llama runtime");
    assert_eq!(payload.metrics.status, "ready");
    assert_eq!(payload.metrics.samples.len(), 2);
    assert_eq!(
        payload.metrics.samples[1]
            .labels
            .as_ref()
            .and_then(|l| l.get("model"))
            .map(String::as_str),
        Some("Qwen3-4B-Q4_K_M")
    );
    assert_eq!(payload.slots.slots.len(), 2);
    assert!(payload.slots.slots[0].is_processing);
    let items = payload.items.expect("items digest");
    assert_eq!(items.slots_total, 2);
    assert_eq!(items.slots_busy, 1);
}

#[test]
fn control_bootstrap_fixture_parses() {
    let raw = include_str!("fixtures/control_bootstrap.json");
    let bootstrap: ControlBootstrap = serde_json::from_str(raw).expect("parse bootstrap");
    assert!(bootstrap.enabled);
    assert!(bootstrap.local_only);
    assert!(bootstrap.endpoint.is_some());
}

#[test]
fn chat_sse_fixture_events_parse() {
    let raw = include_str!("fixtures/responses_sse.txt");
    let mut deltas = String::new();
    let mut reasoning = String::new();
    let mut completed = None;
    let mut other = 0usize;

    for line in raw.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        match serde_json::from_str::<ChatSseEvent>(payload).expect("every event parses") {
            ChatSseEvent::OutputTextDelta { delta, .. } => deltas.push_str(&delta),
            ChatSseEvent::ReasoningTextDelta { delta, .. } => reasoning.push_str(&delta),
            ChatSseEvent::Completed { response } => completed = Some(response),
            ChatSseEvent::Other(_) => other += 1,
        }
    }

    assert_eq!(deltas, "Hello!");
    assert_eq!(reasoning, "thinking…");
    assert_eq!(other, 2, "created + output_text.done fall through to Other");
    let completed = completed.expect("completed event");
    assert_eq!(completed.usage.total_tokens, Some(16));
    assert_eq!(completed.served_by.as_deref(), Some("studio"));
    assert_eq!(completed.timings.unwrap().ttft_ms, Some(118.4));
}

#[test]
fn responses_request_serializes_like_web_console() {
    let request = ResponsesRequest {
        model: "auto".into(),
        client_id: "console-1".into(),
        request_id: "req-1".into(),
        input: vec![ResponsesInputMessage {
            role: "user".into(),
            content: ResponsesContent::Blocks(vec![
                ResponsesContentBlock::Text { text: "hi".into() },
                ResponsesContentBlock::File {
                    url: "obj://abc".into(),
                    mime_type: Some("application/pdf".into()),
                    file_name: Some("doc.pdf".into()),
                },
            ]),
        }],
        stream: true,
        stream_options: Some(StreamOptions {
            include_usage: true,
        }),
    };
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["model"], "auto");
    assert_eq!(json["input"][0]["content"][0]["type"], "input_text");
    assert_eq!(json["input"][0]["content"][1]["type"], "input_file");
    assert_eq!(json["stream_options"]["include_usage"], true);

    // Plain-string content stays a plain string on the wire.
    let simple = ResponsesInputMessage {
        role: "user".into(),
        content: ResponsesContent::Text("hello".into()),
    };
    assert_eq!(serde_json::to_value(&simple).unwrap()["content"], "hello");
}

#[test]
fn unknown_fields_are_tolerated_everywhere() {
    let raw = r#"{
        "node_id": "x",
        "brand_new_field": {"nested": [1,2,3]},
        "peers": [{"id": "p", "another_new_field": true}],
        "gpus": [{"name": "g", "vram_bytes": 1, "future_metric": 9.9}]
    }"#;
    let status: MeshStatus = serde_json::from_str(raw).expect("unknown fields ignored");
    assert_eq!(status.node_id, "x");
    assert_eq!(status.peers.len(), 1);
}
