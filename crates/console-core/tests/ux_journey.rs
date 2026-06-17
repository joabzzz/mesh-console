//! Full user-journey simulation at the logic layer: everything the app does
//! from first-run onboarding through hosting, chatting, configuring, and
//! shutting down — against a real embedded node serving a real model.
//!
//! Heavy (downloads the native runtime + a multi-GB GGUF on first run, both
//! cached afterwards), so it is double-gated:
//!
//! `MESH_CONSOLE_HEAVY_TESTS=1 cargo test -p console-core --test ux_journey -- --ignored --nocapture`
//!
//! Override the model with `MESH_CONSOLE_JOURNEY_MODEL` (default GLM-4.7-Flash).

#![cfg(feature = "embedded")]

use std::time::{Duration, Instant};

use console_core::api::{ApiClient, ChatStreamItem};
use console_core::node_manager::{NodeManager, NodePhase, NodeSpec};
use console_core::vm::{invite, onboarding};
use console_types::{ResponsesContent, ResponsesInputMessage, ResponsesRequest, StreamOptions};

const DEFAULT_MODEL: &str = "unsloth/GLM-4.7-Flash-GGUF:Q4_K_M";
const WARM_TIMEOUT: Duration = Duration::from_secs(3600);

fn step(name: &str) {
    eprintln!("\n━━━ {name} ━━━");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "downloads native runtime + a multi-GB model; set MESH_CONSOLE_HEAVY_TESTS=1"]
async fn full_user_journey() {
    if std::env::var("MESH_CONSOLE_HEAVY_TESTS").as_deref() != Ok("1") {
        eprintln!("skipping: MESH_CONSOLE_HEAVY_TESTS != 1");
        return;
    }
    let model =
        std::env::var("MESH_CONSOLE_JOURNEY_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    // ── 1. Onboarding: hardware detection + model recommendation ────────────
    step("onboarding: detect hardware");
    let hw = onboarding::detect_hardware().expect("GPU detected");
    eprintln!("detected: {} ({:.0} GB)", hw.gpu_name, hw.vram_gb);
    assert!(hw.vram_gb > 0.0);
    let picks = onboarding::recommend_models(hw.vram_gb);
    assert!(!picks.is_empty(), "always at least one starter model");
    eprintln!(
        "starter picks: {:?}",
        picks.iter().map(|m| m.label).collect::<Vec<_>>()
    );

    // ── 2. Create the network (runtime install + model load) ────────────────
    step("create network: start embedded host");
    // SAFETY net for real user data: never persist test node specs.
    unsafe { std::env::set_var("MESH_CONSOLE_EPHEMERAL", "1") };
    let manager = NodeManager::start();
    let handle = manager.handle();
    let mut rx = handle.subscribe();
    handle.start_alt_ports(NodeSpec::host(
        vec![model.clone()],
        Some("ux-journey".to_string()),
        false,
    ));

    let deadline = Instant::now() + WARM_TIMEOUT;
    let console_url = loop {
        let phase = rx.borrow_and_update().clone();
        match &phase {
            NodePhase::InstallingRuntime {
                downloaded_bytes,
                total_bytes,
            } => eprintln!(
                "runtime download: {} / {:?} bytes",
                downloaded_bytes, total_bytes
            ),
            NodePhase::Running { console_url, .. } => break console_url.clone(),
            NodePhase::Failed { message, .. } => panic!("node failed to start: {message}"),
            _ => {}
        }
        assert!(Instant::now() < deadline, "node start timed out");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    eprintln!("node up at {console_url}");

    // ── 3. Wait for the model to be warm (download + load) ──────────────────
    step("host: wait for model warm");
    let api = ApiClient::new(&console_url);
    let status = loop {
        let status = api.fetch_status().await.expect("status");
        if status.serving_models.iter().any(|m| !m.is_empty()) && status.llama_ready {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "model never warmed; serving={:?} ready={}",
            status.serving_models,
            status.llama_ready
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    };
    eprintln!("serving: {:?}", status.serving_models);
    assert_eq!(status.mesh_name.as_deref(), Some("ux-journey"));
    assert!(
        status.owner.as_ref().is_some_and(|o| o.verified),
        "embedded node is owner-verified via the derived keystore"
    );

    // ── 4. Invite surfaces ───────────────────────────────────────────────────
    step("invite: token + commands + QR payload");
    assert!(!status.token.is_empty(), "invite token published in status");
    let with_model = invite::invite_with_model_command(
        &status.token,
        status.serving_models.first().map(|s| s.as_str()),
    )
    .expect("compute command");
    let as_client = invite::invite_client_command(&status.token).expect("client command");
    let api_url = invite::api_direct_url(status.api_port).expect("api url");
    eprintln!("{with_model}\n{as_client}\n{api_url}");

    // ── 5. Model catalog ─────────────────────────────────────────────────────
    step("models: catalog lists the hosted model as warm");
    let models = api.fetch_models().await.expect("models");
    let hosted = models
        .iter()
        .find(|m| m.status == "warm")
        .expect("a warm model in the catalog");
    eprintln!(
        "warm: {} ({} nodes, {:.1} GB)",
        hosted.name, hosted.node_count, hosted.size_gb
    );

    // ── 6. Chat round-trip with streaming + metrics ──────────────────────────
    step("chat: streamed round-trip");
    let request = ResponsesRequest {
        model: "auto".into(),
        client_id: "ux-journey".into(),
        request_id: format!("journey-{}", std::process::id()),
        input: vec![ResponsesInputMessage {
            role: "user".into(),
            content: ResponsesContent::Text(
                "Reply with one short sentence: what is a mesh network?".into(),
            ),
        }],
        stream: true,
        stream_options: Some(StreamOptions {
            include_usage: true,
        }),
    };
    let (tx, mut items) = tokio::sync::mpsc::unbounded_channel();
    api.stream_responses(&request, tx)
        .await
        .expect("responses stream completes");
    let mut text = String::new();
    let mut deltas = 0usize;
    let mut completed = None;
    while let Ok(item) = items.try_recv() {
        match item {
            ChatStreamItem::TextDelta(d) => {
                deltas += 1;
                text.push_str(&d);
            }
            ChatStreamItem::ReasoningDelta(_) => {}
            ChatStreamItem::Completed(c) => completed = Some(c),
        }
    }
    eprintln!("reply ({deltas} deltas): {text}");
    assert!(!text.trim().is_empty(), "model produced text");
    assert!(deltas >= 2, "streaming arrived as multiple deltas");
    let completed = completed.expect("completed event");
    assert!(completed.usage.output_tokens > 0, "usage reported");

    // ── 7. Configuration: bootstrap → get → apply (revision CAS) ────────────
    step("config: control get/apply round-trip");
    let boot = api.fetch_control_bootstrap().await.expect("bootstrap");
    assert!(boot.enabled, "owner-verified node enables config control");
    let snapshot = api
        .control_get_config(boot.endpoint.as_deref())
        .await
        .expect("get-config");
    eprintln!("config revision {}", snapshot.revision);
    let outcome = api
        .control_apply_config(
            boot.endpoint.as_deref(),
            snapshot.revision,
            snapshot.config.clone(),
        )
        .await
        .expect("apply-config");
    eprintln!("apply outcome: {outcome:?}");

    // ── 8. Runtime telemetry the dashboard shows ─────────────────────────────
    // Best-effort: the llama metrics proxy may not serve the embedded skippy
    // backend; what matters is that the call returns (timeout) instead of
    // hanging the dashboard.
    step("dashboard: llama runtime metrics");
    match api.fetch_runtime_llama().await {
        Ok(llama) => eprintln!(
            "metrics: {} samples, slots: {}",
            llama.metrics.samples.len(),
            llama.slots.slots.len()
        ),
        Err(e) => eprintln!("llama metrics unavailable for embedded backend: {e}"),
    }

    // ── 9. Bad join is rejected without downtime ─────────────────────────────
    step("join: malformed token rejected, node stays up");
    let err = handle
        .join("garbage-token".to_string())
        .await
        .expect_err("garbage rejected");
    eprintln!("rejected: {err}");
    assert!(handle.phase().is_running());

    // ── 10. Clean shutdown ───────────────────────────────────────────────────
    step("quit: stop node");
    handle.stop();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if handle.phase() == NodePhase::Idle {
            break;
        }
        assert!(Instant::now() < deadline, "stop timed out");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    manager.shutdown_blocking(Duration::from_secs(15));
    eprintln!("\n━━━ journey complete ━━━");
}
