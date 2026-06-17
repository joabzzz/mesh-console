//! Re-capture JSON fixtures from a live embedded node. Run after every SDK
//! bump so the fixture round-trip tests track the real wire format:
//!
//! `cargo test -p console-core --test capture_fixtures -- --ignored`
//!
//! Writes into `crates/console-types/tests/fixtures/` (client-mode node, so
//! the capture needs no model or native-runtime download).

#![cfg(feature = "embedded")]

use std::time::Duration;

use console_core::node_manager::{NodeManager, NodeMode, NodePhase, NodeSpec};

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../console-types/tests/fixtures")
        .canonicalize()
        .unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../console-types/tests/fixtures")
        })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opens localhost mesh runtime sockets and rewrites fixtures"]
async fn capture_status_and_models_fixtures() {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("create fixtures dir");

    // SAFETY net for real user data: never persist test node specs.
    unsafe { std::env::set_var("MESH_CONSOLE_EPHEMERAL", "1") };
    let manager = NodeManager::start();
    let handle = manager.handle();
    let mut rx = handle.subscribe();
    handle.start_alt_ports(NodeSpec {
        mode: NodeMode::Client,
        models: Vec::new(),
        mesh_name: None,
        publish: false,
        max_vram_gb: None,
        join_tokens: Vec::new(),
    });

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let console_url = loop {
        let phase = rx.borrow_and_update().clone();
        if let NodePhase::Running { console_url, .. } = phase {
            break console_url;
        }
        if let NodePhase::Failed { message, .. } = phase {
            panic!("node failed: {message}");
        }
        assert!(std::time::Instant::now() < deadline, "node start timed out");
        std::thread::sleep(Duration::from_millis(100));
    };

    let http = reqwest::Client::new();
    for (path, file) in [
        ("/api/status", "status_client.json"),
        ("/api/models", "models.json"),
        ("/api/runtime", "runtime.json"),
        ("/api/runtime/processes", "runtime_processes.json"),
        ("/api/runtime/control-bootstrap", "control_bootstrap.json"),
    ] {
        let resp = http
            .get(format!("{console_url}{path}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {path}: {e}"));
        assert!(resp.status().is_success(), "GET {path}: {}", resp.status());
        let value: serde_json::Value = resp.json().await.expect("json body");
        let pretty = serde_json::to_string_pretty(&value).expect("pretty json");
        std::fs::write(dir.join(file), pretty).expect("write fixture");
        println!("captured {file}");
    }

    handle.stop();
    manager.shutdown_blocking(Duration::from_secs(10));
}
