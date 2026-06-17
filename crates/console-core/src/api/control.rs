//! `/api/runtime/control-*` client — owner-gated runtime configuration with
//! compare-and-swap revisions. Same contract as the web console's
//! configuration page (`config-adapter.ts`).

use crate::state::{ApplyConfigRequest, ApplyConfigResponse, ControlError, ControlSnapshot};
use serde::Serialize;

use super::ApiClient;

/// Outcome of an apply: success, a revision conflict carrying the server's
/// current revision (reload + replay), or a hard error.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyOutcome {
    Applied(ApplyConfigResponse),
    RevisionConflict { current_revision: Option<u64> },
}

#[derive(Serialize)]
struct EndpointBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<&'a str>,
}

/// get-config wraps its payload (`{"snapshot": …}`); apply-config does not.
#[derive(serde::Deserialize)]
struct SnapshotEnvelope {
    snapshot: ControlSnapshot,
}

impl ApiClient {
    async fn post_control<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
        what: &str,
    ) -> Result<T, String> {
        let resp = self
            .http()
            .post(self.url(path))
            .timeout(std::time::Duration::from_secs(15))
            .json(body)
            .send()
            .await
            .map_err(|e| format!("{what} request failed: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ControlError>(&text) {
                if !err.message.is_empty() {
                    return Err(format!("{what}: {} ({})", err.message, err.code));
                }
            }
            return Err(format!("{what} HTTP {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("{what} parse error: {e}"))
    }

    /// Fetch the current config snapshot (revision + JSON config blob).
    pub async fn control_get_config(
        &self,
        endpoint: Option<&str>,
    ) -> Result<ControlSnapshot, String> {
        let envelope: SnapshotEnvelope = self
            .post_control(
                "/api/runtime/control/get-config",
                &EndpointBody { endpoint },
                "get-config",
            )
            .await?;
        Ok(envelope.snapshot)
    }

    /// Apply a config against `expected_revision`. A stale revision comes
    /// back as [`ApplyOutcome::RevisionConflict`] rather than `Err`, so the
    /// UI can reload and let the user retry.
    pub async fn control_apply_config(
        &self,
        endpoint: Option<&str>,
        expected_revision: u64,
        config: serde_json::Value,
    ) -> Result<ApplyOutcome, String> {
        let body = ApplyConfigRequest {
            endpoint: endpoint.map(str::to_string),
            expected_revision,
            // Serialized MeshConfig carries `null` for unset Options, but the
            // daemon's deserializer (TOML semantics) rejects null outright.
            config: crate::vm::config_history::strip_nulls(&config),
        };
        let resp = self
            .http()
            .post(self.url("/api/runtime/control/apply-config"))
            .timeout(std::time::Duration::from_secs(15))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("apply-config request failed: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if status.is_success() {
            let applied: ApplyConfigResponse = serde_json::from_str(&text)
                .map_err(|e| format!("apply-config parse error: {e}"))?;
            if applied.success {
                return Ok(ApplyOutcome::Applied(applied));
            }
            return Ok(ApplyOutcome::RevisionConflict {
                current_revision: Some(applied.current_revision),
            });
        }

        if let Ok(err) = serde_json::from_str::<ControlError>(&text) {
            if err.code.contains("revision") || err.current_revision.is_some() {
                return Ok(ApplyOutcome::RevisionConflict {
                    current_revision: err.current_revision,
                });
            }
            if !err.message.is_empty() {
                return Err(format!("apply-config: {} ({})", err.message, err.code));
            }
        }
        Err(format!("apply-config HTTP {status}: {text}"))
    }
}
