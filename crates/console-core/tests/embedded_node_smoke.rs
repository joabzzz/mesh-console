//! Real embedded-node lifecycle test (no model, no native-runtime download):
//! starts a client-mode node on free ports through the NodeManager, checks the
//! management API answers and our types parse it, exercises the join error
//! path, and stops cleanly.
//!
//! Ignored by default because it opens localhost sockets (same convention as
//! the SDK's own embedded test). Run with:
//! `cargo test -p console-core --test embedded_node_smoke -- --ignored`

#![cfg(feature = "embedded")]

use std::time::{Duration, Instant};

use console_core::node_manager::{NodeManager, NodeMode, NodePhase, NodeSpec};

const PHASE_TIMEOUT: Duration = Duration::from_secs(30);

fn wait_for_phase(
    rx: &mut tokio::sync::watch::Receiver<NodePhase>,
    pred: impl Fn(&NodePhase) -> bool,
) -> NodePhase {
    let deadline = Instant::now() + PHASE_TIMEOUT;
    loop {
        let current = rx.borrow_and_update().clone();
        if pred(&current) {
            return current;
        }
        if let NodePhase::Failed { message, .. } = &current {
            panic!("node entered Failed while waiting: {message}");
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for node phase; last = {current:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opens localhost mesh runtime sockets"]
async fn embedded_client_lifecycle_through_manager() {
    // SAFETY net for real user data: never persist test node specs.
    unsafe { std::env::set_var("MESH_CONSOLE_EPHEMERAL", "1") };
    let manager = NodeManager::start();
    let handle = manager.handle();
    let mut rx = handle.subscribe();

    // Client mode loads no model, so this needs no native runtime download.
    // Alternate ports keep the test independent of anything on 3131/9337.
    handle.start_alt_ports(NodeSpec {
        mode: NodeMode::Client,
        models: Vec::new(),
        mesh_name: None,
        publish: false,
        max_vram_gb: None,
        join_tokens: Vec::new(),
    });

    let running = wait_for_phase(&mut rx, NodePhase::is_running);
    let NodePhase::Running {
        console_url,
        api_base_url,
        console_port,
        api_port,
        ..
    } = running
    else {
        unreachable!()
    };
    assert_eq!(console_url, format!("http://127.0.0.1:{console_port}"));
    assert_eq!(api_base_url, format!("http://127.0.0.1:{api_port}/v1"));

    // The management API must answer and parse into our MeshStatus mirror.
    console_core::api::set_api_base(console_url.clone());
    let status = console_core::api::fetch_status()
        .await
        .expect("fetch /api/status from embedded node");
    assert!(
        !status.node_id.is_empty(),
        "embedded node reports a node id"
    );

    // Live-join with a malformed token must fail without taking the node down.
    let err = handle
        .join("not-a-real-invite-token".to_string())
        .await
        .expect_err("garbage invite token should be rejected");
    assert!(!err.is_empty());
    assert!(handle.phase().is_running(), "node survives a bad join");

    handle.stop();
    wait_for_phase(&mut rx, |p| *p == NodePhase::Idle);

    // Port is released once the runtime thread exits.
    let deadline = Instant::now() + PHASE_TIMEOUT;
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", console_port)).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "console port {console_port} still bound after stop"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    manager.shutdown_blocking(Duration::from_secs(10));
}
