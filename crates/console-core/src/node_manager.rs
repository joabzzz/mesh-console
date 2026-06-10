//! Lifecycle owner for the in-process mesh-llm node.
//!
//! The manager runs on its own thread with its own tokio runtime (the
//! embedded mesh runtime additionally spawns its own thread inside the SDK).
//! The UI talks to it through [`NodeManagerHandle`]: commands go over an
//! unbounded mpsc channel, the current [`NodePhase`] comes back over a watch
//! channel that the Dioxus app bridges into a signal.
//!
//! Replaces the old `process.rs` approach of forking a `mesh-llm` binary and
//! pkill-ing it: the node lives and dies with the app, no CLI install needed.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mesh_llm_sdk::embedded_runtime::EmbeddedMeshNodeHandle;
use mesh_llm_sdk::MeshNode;
use tokio::sync::{mpsc, oneshot, watch};

use crate::runtime_install;

pub const DEFAULT_API_PORT: u16 = 9337;
pub const DEFAULT_CONSOLE_PORT: u16 = 3131;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const KEYSTORE_FILE: &str = "owner-keystore.json";

/// How the embedded node participates in a mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeMode {
    /// Contribute hardware: host/serve models (`mesh-llm serve`).
    Serve,
    /// Route-only client (`mesh-llm client`).
    Client,
}

/// Everything needed to start the embedded node.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSpec {
    pub mode: NodeMode,
    /// Model refs to serve (serve mode; may be empty when joining a mesh that
    /// assigns models).
    pub models: Vec<String>,
    pub mesh_name: Option<String>,
    /// Publish the mesh for Nostr discovery.
    pub publish: bool,
    pub max_vram_gb: Option<f64>,
    /// Invite tokens to join at startup.
    pub join_tokens: Vec<String>,
}

impl NodeSpec {
    pub fn host(models: Vec<String>, mesh_name: Option<String>, publish: bool) -> Self {
        Self {
            mode: NodeMode::Serve,
            models,
            mesh_name,
            publish,
            max_vram_gb: None,
            join_tokens: Vec::new(),
        }
    }

    /// Join an existing mesh as a hardware contributor (the equivalent of
    /// `mesh-llm serve --join <token>`).
    pub fn join(token: String) -> Self {
        Self {
            mode: NodeMode::Serve,
            models: Vec::new(),
            mesh_name: None,
            publish: false,
            max_vram_gb: None,
            join_tokens: vec![token],
        }
    }
}

#[derive(Debug)]
pub enum NodeCommand {
    /// Start the embedded node (stops a previously started one first).
    Start { spec: NodeSpec, alt_ports: bool },
    /// Join a mesh on the already-running node.
    Join {
        token: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Stop the embedded node, back to Idle.
    Stop,
    /// Stop the node and exit the manager thread (app quit).
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Another mesh-llm daemon already answers on the default console port.
    ExternalDaemon,
    /// A non-mesh-llm process occupies one of the ports we need.
    PortConflict,
    RuntimeInstall,
    Startup,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum NodePhase {
    #[default]
    Idle,
    InstallingRuntime {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Starting,
    Running {
        console_url: String,
        api_base_url: String,
        invite_token: Option<String>,
        api_port: u16,
        console_port: u16,
    },
    Stopping,
    Failed {
        message: String,
        kind: FailureKind,
    },
}

impl NodePhase {
    pub fn is_running(&self) -> bool {
        matches!(self, NodePhase::Running { .. })
    }

    /// True while a start request is in flight (install or boot).
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            NodePhase::InstallingRuntime { .. } | NodePhase::Starting | NodePhase::Stopping
        )
    }
}

/// Cloneable handle the UI uses to drive the node manager.
#[derive(Clone)]
pub struct NodeManagerHandle {
    cmd_tx: mpsc::UnboundedSender<NodeCommand>,
    phase_rx: watch::Receiver<NodePhase>,
}

impl NodeManagerHandle {
    pub fn phase(&self) -> NodePhase {
        self.phase_rx.borrow().clone()
    }

    /// Fresh receiver for awaiting phase changes.
    pub fn subscribe(&self) -> watch::Receiver<NodePhase> {
        self.phase_rx.clone()
    }

    /// Fire-and-forget start; progress and errors surface through the phase.
    pub fn start(&self, spec: NodeSpec) {
        let _ = self.cmd_tx.send(NodeCommand::Start {
            spec,
            alt_ports: false,
        });
    }

    /// Start on automatically-chosen free ports (after an `ExternalDaemon` /
    /// `PortConflict` failure).
    pub fn start_alt_ports(&self, spec: NodeSpec) {
        let _ = self.cmd_tx.send(NodeCommand::Start {
            spec,
            alt_ports: true,
        });
    }

    /// Join a mesh on the running node. Resolves with the runtime's answer.
    pub async fn join(&self, token: String) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(NodeCommand::Join { token, reply })
            .map_err(|_| "node manager is gone".to_string())?;
        rx.await
            .map_err(|_| "node manager dropped the join request".to_string())?
    }

    pub fn stop(&self) {
        let _ = self.cmd_tx.send(NodeCommand::Stop);
    }
}

/// Owns the manager thread. Created in `main()` before the UI launches;
/// `shutdown_blocking` runs after the window closes.
pub struct NodeManager {
    handle: NodeManagerHandle,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl NodeManager {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (phase_tx, phase_rx) = watch::channel(NodePhase::Idle);
        let thread = std::thread::Builder::new()
            .name("mesh-console-node-manager".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("build node manager runtime");
                runtime.block_on(run_manager(cmd_rx, Arc::new(phase_tx)));
            })
            .expect("spawn node manager thread");
        Self {
            handle: NodeManagerHandle { cmd_tx, phase_rx },
            thread: Some(thread),
        }
    }

    pub fn handle(&self) -> NodeManagerHandle {
        self.handle.clone()
    }

    /// Stop the embedded node and wait (bounded) for the manager thread to
    /// exit. Called after the UI event loop returns; on timeout we just exit —
    /// process teardown is the fallback.
    pub fn shutdown_blocking(mut self, timeout: Duration) {
        let _ = self.handle.cmd_tx.send(NodeCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let deadline = std::time::Instant::now() + timeout;
            while !thread.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

async fn run_manager(
    mut cmd_rx: mpsc::UnboundedReceiver<NodeCommand>,
    phase_tx: Arc<watch::Sender<NodePhase>>,
) {
    let mut node: Option<EmbeddedMeshNodeHandle> = None;

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NodeCommand::Start { spec, alt_ports } => {
                if let Some(existing) = node.take() {
                    let _ = phase_tx.send(NodePhase::Stopping);
                    let _ = existing.stop().await;
                }
                match start_node(spec.clone(), alt_ports, &phase_tx).await {
                    Ok(handle) => {
                        remember_last_node(&spec);
                        let _ = phase_tx.send(NodePhase::Running {
                            console_url: handle.console_url().to_string(),
                            api_base_url: handle.api_base_url().to_string(),
                            invite_token: handle.invite_token().map(str::to_string),
                            api_port: port_from_url(handle.api_base_url())
                                .unwrap_or(DEFAULT_API_PORT),
                            console_port: port_from_url(handle.console_url())
                                .unwrap_or(DEFAULT_CONSOLE_PORT),
                        });
                        node = Some(handle);
                    }
                    Err((message, kind)) => {
                        let _ = phase_tx.send(NodePhase::Failed { message, kind });
                    }
                }
            }
            NodeCommand::Join { token, reply } => {
                let result = match node.as_ref() {
                    Some(handle) => handle
                        .join_token(token)
                        .await
                        .map_err(|e| format!("join failed: {e:#}")),
                    None => Err("no running node — start or join a mesh first".to_string()),
                };
                let _ = reply.send(result);
            }
            NodeCommand::Stop => {
                if let Some(handle) = node.take() {
                    let _ = phase_tx.send(NodePhase::Stopping);
                    if let Err(e) = handle.stop().await {
                        let _ = phase_tx.send(NodePhase::Failed {
                            message: format!("stop failed: {e:#}"),
                            kind: FailureKind::Startup,
                        });
                        continue;
                    }
                }
                let _ = phase_tx.send(NodePhase::Idle);
            }
            NodeCommand::Shutdown => {
                if let Some(handle) = node.take() {
                    let _ = phase_tx.send(NodePhase::Stopping);
                    let _ = handle.stop().await;
                }
                let _ = phase_tx.send(NodePhase::Idle);
                break;
            }
        }
    }
}

async fn start_node(
    spec: NodeSpec,
    alt_ports: bool,
    phase_tx: &Arc<watch::Sender<NodePhase>>,
) -> Result<EmbeddedMeshNodeHandle, (String, FailureKind)> {
    let (api_port, console_port) = if alt_ports {
        (
            free_local_port().map_err(|e| (e, FailureKind::PortConflict))?,
            free_local_port().map_err(|e| (e, FailureKind::PortConflict))?,
        )
    } else {
        preflight_default_ports().await?;
        (DEFAULT_API_PORT, DEFAULT_CONSOLE_PORT)
    };

    // Serve mode loads models through the dynamic native runtime; make sure
    // one is installed before booting (client mode never needs it).
    if spec.mode == NodeMode::Serve && !runtime_install::native_runtime_installed() {
        let _ = phase_tx.send(NodePhase::InstallingRuntime {
            downloaded_bytes: 0,
            total_bytes: None,
        });
        let progress_tx = Arc::clone(phase_tx);
        runtime_install::ensure_native_runtime(move |p| {
            let _ = progress_tx.send(NodePhase::InstallingRuntime {
                downloaded_bytes: p.downloaded_bytes,
                total_bytes: p.total_bytes,
            });
        })
        .await
        .map_err(|e| (e, FailureKind::RuntimeInstall))?;
    }

    let _ = phase_tx.send(NodePhase::Starting);

    let keystore = ensure_owner_keystore().map_err(|e| (e, FailureKind::Startup))?;

    let mut builder = MeshNode::builder()
        .console_ui(false)
        .api_port(api_port)
        .console_port(console_port)
        .owner_key(keystore)
        .startup_timeout(STARTUP_TIMEOUT);

    builder = match spec.mode {
        NodeMode::Serve => builder.serve(),
        NodeMode::Client => builder.client(),
    };
    if !spec.models.is_empty() {
        builder = builder.models(spec.models.clone());
    }
    if let Some(name) = spec.mesh_name.as_deref().filter(|n| !n.is_empty()) {
        builder = builder.mesh_name(name);
    }
    if spec.publish {
        builder = builder.publish(true);
    }
    if let Some(vram) = spec.max_vram_gb {
        builder = builder.max_vram_gb(vram);
    }
    if !spec.join_tokens.is_empty() {
        builder = builder.join_tokens(spec.join_tokens.clone());
    }

    let node = builder
        .start()
        .await
        .map_err(|e| (format!("node start failed: {e:#}"), FailureKind::Startup))?;
    Ok(node.into_inner())
}

/// Check the default ports are free; classify what's in the way if not.
async fn preflight_default_ports() -> Result<(), (String, FailureKind)> {
    for port in [DEFAULT_CONSOLE_PORT, DEFAULT_API_PORT] {
        if port_free(port) {
            continue;
        }
        if let Some(version) = external_daemon_version(DEFAULT_CONSOLE_PORT).await {
            return Err((
                format!(
                    "another mesh-llm daemon (v{version}) is already running on this machine \
                     (port {DEFAULT_CONSOLE_PORT}). Stop it, or start the console node on \
                     alternate ports."
                ),
                FailureKind::ExternalDaemon,
            ));
        }
        return Err((
            format!("port {port} is in use by another application"),
            FailureKind::PortConflict,
        ));
    }
    Ok(())
}

/// Probe `/api/status` on the given console port; `Some(version)` when a
/// mesh-llm daemon answers.
async fn external_daemon_version(console_port: u16) -> Option<String> {
    let url = format!("http://127.0.0.1:{console_port}/api/status");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    let payload = client
        .get(&url)
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;
    payload.get("node_id")?;
    Some(
        payload
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
    )
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn free_local_port() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|addr| addr.port())
        .map_err(|e| format!("could not allocate a free port: {e}"))
}

fn port_from_url(url: &str) -> Option<u16> {
    let after_scheme = url.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next()?;
    host_port.rsplit(':').next()?.parse().ok()
}

/// Persist the spec that successfully started, so relaunches can offer the
/// same setup (and onboarding knows it already happened). Best-effort.
fn remember_last_node(spec: &NodeSpec) {
    let mut settings = crate::settings::load();
    settings.last_node = Some(crate::settings::LastNodeConfig {
        mode: match spec.mode {
            NodeMode::Serve => "serve".to_string(),
            NodeMode::Client => "client".to_string(),
        },
        models: spec.models.clone(),
        mesh_name: spec.mesh_name.clone(),
        publish: spec.publish,
        join_tokens: spec.join_tokens.clone(),
        max_vram_gb: spec.max_vram_gb,
    });
    let _ = crate::settings::save(&settings);
}

/// The embedded node wants its owner identity as a mesh-llm keystore file,
/// while the chat client SDK uses the raw hex keypair at
/// `<config_dir>/mesh-console/owner.hex`. Derive the keystore from the same
/// keypair so both surfaces present one owner identity.
fn ensure_owner_keystore() -> Result<PathBuf, String> {
    let keypair = crate::sdk_session::load_or_create_owner_keypair()?;
    let path = dirs::config_dir()
        .map(|d| d.join("mesh-console").join(KEYSTORE_FILE))
        .ok_or_else(|| "could not resolve user config directory".to_string())?;
    if path.exists() {
        return Ok(path);
    }
    let identity = mesh_llm_identity::OwnerKeypair::from_bytes(
        keypair.signing_bytes(),
        &keypair.encryption_bytes(),
    )
    .map_err(|e| format!("derive owner identity: {e}"))?;
    mesh_llm_identity::keystore::save_keystore(&path, &identity, None, false)
        .map_err(|e| format!("write owner keystore {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_from_url_extracts_ports() {
        assert_eq!(port_from_url("http://127.0.0.1:9337/v1"), Some(9337));
        assert_eq!(port_from_url("http://127.0.0.1:3131"), Some(3131));
        assert_eq!(port_from_url("not a url"), None);
    }

    #[test]
    fn node_spec_join_is_serve_mode() {
        let spec = NodeSpec::join("token".to_string());
        assert_eq!(spec.mode, NodeMode::Serve);
        assert!(spec.models.is_empty());
        assert_eq!(spec.join_tokens, vec!["token".to_string()]);
    }
}
