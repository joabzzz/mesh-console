//! `/api/runtime/control-*` — owner-gated runtime configuration control.
//! Mirrors the payloads in `mesh-llm-host-runtime/src/api/routes/runtime.rs`
//! and `api/state.rs`.
//!
//! The `config` blob is kept as raw JSON here; `console-core` (with the
//! `embedded` feature) converts it to/from the real `MeshConfig` type
//! re-exported by the SDK, which is also what the TOML editor round-trips.

use serde::{Deserialize, Serialize};

/// `GET /api/runtime/control-bootstrap` — is config control available, and
/// through which endpoint?
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlBootstrap {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub local_only: bool,
    #[serde(default)]
    pub requires_explicit_remote_endpoint: bool,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub suggested_commands: Option<Vec<String>>,
}

/// `POST /api/runtime/control/get-config` response.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlSnapshot {
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub config_hash: String,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

/// `POST /api/runtime/control/apply-config` request body. `expected_revision`
/// makes applies compare-and-swap: a stale revision is rejected with
/// [`ControlError::current_revision`] set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApplyConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub expected_revision: u64,
    pub config: serde_json::Value,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyConfigResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub current_revision: u64,
    #[serde(default)]
    pub config_hash: String,
    #[serde(default)]
    pub apply_mode: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// Error body for control endpoints (HTTP 4xx/5xx).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlError {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub legacy_retry_allowed: bool,
    #[serde(default)]
    pub current_revision: Option<u64>,
}
