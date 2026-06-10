//! ApiClient against a tiny in-test HTTP server (no embedded runtime needed):
//! happy paths over real fixtures, HTTP error mapping, malformed JSON, and
//! SSE streaming incl. partial frames. Runs in the default test tier.

use std::collections::HashMap;
use std::sync::Arc;

use console_core::api::ApiClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const STATUS_FULL: &str = include_str!("../../console-types/tests/fixtures/status_full.json");
const MODELS_FULL: &str = include_str!("../../console-types/tests/fixtures/models_full.json");
const RUNTIME_LLAMA: &str = include_str!("../../console-types/tests/fixtures/runtime_llama.json");

/// Routes: path → (status_code, body). Anything else 404s.
async fn spawn_mock(routes: HashMap<&'static str, (u16, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let routes = Arc::new(routes);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let routes = Arc::clone(&routes);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (code, body) = routes
                    .get(path.as_str())
                    .cloned()
                    .unwrap_or((404, "not found".to_string()));
                let reason = match code {
                    200 => "OK",
                    400 => "Bad Request",
                    500 => "Internal Server Error",
                    _ => "Not Found",
                };
                let response = format!(
                    "HTTP/1.1 {code} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn fetch_status_models_and_llama_parse_fixtures() {
    let base = spawn_mock(HashMap::from([
        ("/api/status", (200, STATUS_FULL.to_string())),
        ("/api/models", (200, MODELS_FULL.to_string())),
        ("/api/runtime/llama", (200, RUNTIME_LLAMA.to_string())),
    ]))
    .await;
    let client = ApiClient::new(&base);

    let status = client.fetch_status().await.expect("status");
    assert_eq!(status.node_state, "serving");
    assert_eq!(status.peers.len(), 2);

    let models = client.fetch_models().await.expect("models");
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].status, "warm");

    let llama = client.fetch_runtime_llama().await.expect("llama");
    assert_eq!(llama.items.unwrap().slots_busy, 1);
}

#[tokio::test]
async fn http_errors_and_bad_json_map_to_readable_messages() {
    let base = spawn_mock(HashMap::from([
        ("/api/status", (500, "{}".to_string())),
        ("/api/models", (200, "{not json".to_string())),
    ]))
    .await;
    let client = ApiClient::new(&base);

    let err = client.fetch_status().await.expect_err("500 surfaces");
    assert!(err.contains("500"), "got: {err}");

    let err = client.fetch_models().await.expect_err("bad json surfaces");
    assert!(err.contains("parse error"), "got: {err}");

    let err = client
        .fetch_runtime_processes()
        .await
        .expect_err("404 surfaces");
    assert!(err.contains("404"), "got: {err}");
}

#[tokio::test]
async fn load_model_posts_and_surfaces_error_body() {
    let base = spawn_mock(HashMap::from([(
        "/api/runtime/models",
        (400, r#"{"error":"model not found"}"#.to_string()),
    )]))
    .await;
    let client = ApiClient::new(&base);
    let err = client.load_model("nope").await.expect_err("400 surfaces");
    assert!(
        err.contains("400") && err.contains("model not found"),
        "got: {err}"
    );
}

#[tokio::test]
async fn connection_refused_is_an_error_not_a_panic() {
    // Port from a listener we immediately drop — nothing is listening.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let client = ApiClient::new(format!("http://127.0.0.1:{port}"));
    let err = client.fetch_status().await.expect_err("refused");
    assert!(err.contains("request failed"), "got: {err}");
}

/// SSE endpoint streaming two status frames split across writes, then a clean
/// close: both frames must arrive, then the stream ends Ok.
#[tokio::test]
async fn stream_status_parses_sse_frames_across_chunks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = socket.read(&mut buf).await;
        let head =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        socket.write_all(head.as_bytes()).await.unwrap();
        // First frame, then a frame deliberately split mid-payload.
        socket
            .write_all(b"data: {\"node_id\":\"one\"}\n\ndata: {\"node_")
            .await
            .unwrap();
        socket.flush().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        socket.write_all(b"id\":\"two\"}\n\n").await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let client = ApiClient::new(format!("http://{addr}"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    client.stream_status(tx).await.expect("clean close");

    let mut ids = Vec::new();
    while let Ok(status) = rx.try_recv() {
        ids.push(status.node_id);
    }
    assert_eq!(ids, vec!["one".to_string(), "two".to_string()]);
}

/// `/api/responses`: serve the canonical SSE fixture; the digested stream
/// must carry the deltas, the reasoning, and the completed metadata.
#[tokio::test]
async fn stream_responses_digests_fixture_events() {
    const SSE_BODY: &str = include_str!("../../console-types/tests/fixtures/responses_sse.txt");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 16384];
        let _ = socket.read(&mut buf).await;
        let head =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(SSE_BODY.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let client = ApiClient::new(format!("http://{addr}"));
    let request = console_types::ResponsesRequest {
        model: "auto".into(),
        client_id: "test".into(),
        request_id: "req-1".into(),
        input: vec![console_types::ResponsesInputMessage {
            role: "user".into(),
            content: console_types::ResponsesContent::Text("hi".into()),
        }],
        stream: true,
        stream_options: Some(console_types::StreamOptions {
            include_usage: true,
        }),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    client
        .stream_responses(&request, tx)
        .await
        .expect("stream completes");

    use console_core::api::ChatStreamItem;
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut completed = None;
    while let Ok(item) = rx.try_recv() {
        match item {
            ChatStreamItem::TextDelta(d) => text.push_str(&d),
            ChatStreamItem::ReasoningDelta(d) => reasoning.push_str(&d),
            ChatStreamItem::Completed(c) => completed = Some(c),
        }
    }
    assert_eq!(text, "Hello!");
    assert_eq!(reasoning, "thinking…");
    let completed = completed.expect("completed item");
    assert_eq!(completed.served_by.as_deref(), Some("studio"));
    assert_eq!(completed.usage.input_tokens, 12);
}

/// A stream that dies before `response.completed` must surface an error.
#[tokio::test]
async fn stream_responses_errors_on_truncated_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 16384];
        let _ = socket.read(&mut buf).await;
        let head =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        socket.write_all(head.as_bytes()).await.unwrap();
        socket
            .write_all(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"par\"}\n\n")
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
    });

    let client = ApiClient::new(format!("http://{addr}"));
    let request = console_types::ResponsesRequest {
        model: "auto".into(),
        client_id: "test".into(),
        request_id: "req-2".into(),
        input: vec![],
        stream: true,
        stream_options: None,
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let err = client
        .stream_responses(&request, tx)
        .await
        .expect_err("truncated stream errors");
    assert!(err.contains("before completion"), "got: {err}");
}

/// Control endpoints: snapshot fetch + revision-conflict mapping.
#[tokio::test]
async fn control_get_and_apply_with_revision_conflict() {
    use console_core::api::ApplyOutcome;
    let snapshot = r#"{"node_id":"n1","revision":7,"config_hash":"h","config":{"version":1}}"#;
    let conflict = r#"{"code":"revision_mismatch","message":"stale revision","legacy_retry_allowed":false,"current_revision":9}"#;
    let base = spawn_mock(HashMap::from([
        (
            "/api/runtime/control/get-config",
            (200, snapshot.to_string()),
        ),
        (
            "/api/runtime/control/apply-config",
            (409, conflict.to_string()),
        ),
    ]))
    .await;
    let client = ApiClient::new(&base);

    let snap = client
        .control_get_config(Some("endpoint-token"))
        .await
        .expect("snapshot");
    assert_eq!(snap.revision, 7);
    assert_eq!(snap.config["version"], 1);

    let outcome = client
        .control_apply_config(Some("endpoint-token"), 7, serde_json::json!({"version": 1}))
        .await
        .expect("conflict is not a hard error");
    assert_eq!(
        outcome,
        ApplyOutcome::RevisionConflict {
            current_revision: Some(9)
        }
    );
}

#[tokio::test]
async fn control_apply_success_path() {
    use console_core::api::ApplyOutcome;
    let applied = r#"{"success":true,"current_revision":8,"config_hash":"h2","apply_mode":"live"}"#;
    let base = spawn_mock(HashMap::from([(
        "/api/runtime/control/apply-config",
        (200, applied.to_string()),
    )]))
    .await;
    let client = ApiClient::new(&base);
    let outcome = client
        .control_apply_config(None, 7, serde_json::json!({"version": 1}))
        .await
        .expect("applied");
    match outcome {
        ApplyOutcome::Applied(resp) => {
            assert_eq!(resp.current_revision, 8);
            assert_eq!(resp.apply_mode, "live");
        }
        other => panic!("expected Applied, got {other:?}"),
    }
}
