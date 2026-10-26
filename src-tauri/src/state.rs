use crate::events::AppEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Host,
    Join,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Public,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum Phase {
    Idle,
    InstallingRuntime,
    Downloading { model: String },
    Starting { mode: Mode, model: Option<String> },
    Running(RunningInfo),
    Error { message: String, recoverable: bool },
}

#[derive(Clone, Debug, Serialize)]
pub struct RunningInfo {
    pub mode: Mode,
    pub visibility: Visibility,
    pub model: Option<String>,
    pub serving: bool,
    pub invite_token: Option<String>,
    pub api_port: u16,
    pub console_port: u16,
    pub mesh_name: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct Ports {
    pub app: u16,
    pub api: u16,
    pub console: u16,
}

pub struct AppState {
    pub compute: Option<Arc<crate::compute::Compute>>,
    pub phase: RwLock<Phase>,
    pub node: Mutex<Option<mesh_llm_sdk::MeshNode>>,
    /// Embedded goose agent, created lazily on the first chat turn.
    pub agent: Mutex<Option<crate::agent::AgentHandle>>,
    pub events: broadcast::Sender<AppEvent>,
    pub ports: Ports,
    pub http: reqwest::Client,
    /// Invite token from a `mesh://join/<token>` deep link, parked until the
    /// frontend picks it up (one-shot via GET /app/pending_invite). std Mutex
    /// (not tokio): the deep-link handler in main.rs is sync, and the critical
    /// section is a plain Option swap.
    pub pending_invite: std::sync::Mutex<Option<String>>,
}

impl AppState {
    pub fn new(ports: Ports) -> Arc<Self> {
        let (events, _) = broadcast::channel(512);
        Arc::new(Self {
            compute: crate::compute::Compute::from_env()
                .expect("Invalid explicit m8s desktop configuration"),
            phase: RwLock::new(Phase::Idle),
            node: Mutex::new(None),
            agent: Mutex::new(None),
            events,
            ports,
            http: reqwest::Client::new(),
            pending_invite: std::sync::Mutex::new(None),
        })
    }

    pub async fn phase(&self) -> Phase {
        self.phase.read().await.clone()
    }

    pub async fn set_phase(&self, phase: Phase) {
        *self.phase.write().await = phase.clone();
        let _ = self.events.send(AppEvent::Phase(phase));
    }

    /// Mutate the Running info in place (e.g. a late-arriving invite token)
    /// and rebroadcast the phase so connected UIs converge.
    pub async fn update_running(&self, f: impl FnOnce(&mut RunningInfo)) {
        let mut guard = self.phase.write().await;
        if let Phase::Running(info) = &mut *guard {
            f(info);
            let _ = self.events.send(AppEvent::Phase(guard.clone()));
        }
    }
}
