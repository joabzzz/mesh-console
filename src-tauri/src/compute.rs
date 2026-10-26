//! Explicit, single-attempt desktop adapter for m8s's certified demo owner.
//! No provider credentials, signing keys, shell commands or CREATE retries.
use crate::state::AppState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

pub const MODEL: &str =
    "unsloth/Qwen3.8-27B-GGUF@4ca720788d1e01f1bff70c033e0d0028fd02e502:UD-Q4_K_M";
const RESPONSE_MODEL: &str =
    "local-gguf/sha256-7756e8943d5ec98b1cd76895d33d20b8c8bf7609a71540fc9b6fa512bdedd0de";
const ERROR: &str = "Compute unavailable; retained state is not proof of removal";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    cli: PathBuf,
    control_directory: PathBuf,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema: String,
    api_port: u16,
    console_port: u16,
    node_id: String,
    owner_id: String,
    cert_id: String,
    expires_at: i64,
}
pub struct Compute {
    config: Config,
    attempt: PathBuf,
    cap: u64,
    image: String,
    account: u64,
    http: reqwest::Client,
    action: Mutex<()>,
    history: Mutex<Vec<Value>>,
}
fn read_private(path: &Path) -> anyhow::Result<Value> {
    anyhow::ensure!(path.is_absolute() && path.canonicalize()? == path, ERROR);
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!(ERROR))?;
    let pm = std::fs::metadata(parent)?;
    anyhow::ensure!(
        pm.uid() == unsafe { libc::geteuid() } && pm.mode() & 0o777 == 0o700,
        ERROR
    );
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let m = f.metadata()?;
    anyhow::ensure!(
        m.is_file()
            && m.uid() == unsafe { libc::geteuid() }
            && m.mode() & 0o777 == 0o600
            && m.nlink() == 1
            && m.len() <= 32768,
        ERROR
    );
    let mut bytes = Vec::new();
    f.take(32769).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 32768, ERROR);
    Ok(serde_json::from_slice(&bytes)?)
}
fn allowance(plan: &Value) -> anyhow::Result<u64> {
    let total = plan["policy"]["trial_all_in"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!(ERROR))?;
    let baseline = plan["policy"]["baseline"]["prior_lifetime_spent"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!(ERROR))?;
    let cap = total
        .checked_sub(baseline)
        .filter(|n| (1..=5_000_000).contains(n))
        .ok_or_else(|| anyhow::anyhow!(ERROR))?;
    anyhow::ensure!(
        plan["model"] == "qwen3.8-27b"
            && plan["tier"] == "qwen38-27b"
            && plan["schema"] == "m8s.vast-demo/v1",
        ERROR
    );
    Ok(cap)
}
impl Compute {
    pub fn from_env() -> anyhow::Result<Option<Arc<Self>>> {
        let Some(path) = std::env::var_os("M8S_DESKTOP_CONFIG") else {
            return Ok(None);
        };
        let config: Config = serde_json::from_value(read_private(Path::new(&path))?)?;
        anyhow::ensure!(
            config.cli.is_absolute() && config.cli.canonicalize()? == config.cli,
            ERROR
        );
        let m = std::fs::metadata(&config.cli)?;
        anyhow::ensure!(
            m.is_file()
                && m.uid() == unsafe { libc::geteuid() }
                && m.mode() & 0o022 == 0
                && m.mode() & 0o111 != 0,
            ERROR
        );
        let plan = read_private(&config.control_directory.join("launch-plan.json"))?;
        anyhow::ensure!(
            plan["control_directory"].as_str() == config.control_directory.to_str(),
            ERROR
        );
        let cap = allowance(&plan)?;
        let attempt = PathBuf::from(
            plan["directory"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(ERROR))?,
        );
        anyhow::ensure!(attempt.is_absolute(), ERROR);
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(65))
            .build()?;
        Ok(Some(Arc::new(Self {
            config,
            attempt,
            cap,
            image: plan["image"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(ERROR))?
                .to_owned(),
            account: plan["expected_account_id"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!(ERROR))?,
            http,
            action: Mutex::new(()),
            history: Mutex::new(vec![]),
        })))
    }
    pub async fn request_down(&self) -> anyhow::Result<()> {
        self.command("down").await?;
        Ok(())
    }
    async fn command(&self, verb: &str) -> anyhow::Result<Value> {
        let mut command = tokio::process::Command::new(&self.config.cli);
        command
            .arg("--socket")
            .arg(self.config.control_directory.join("control.sock"))
            .arg(verb);
        if verb == "up" {
            command.arg("qwen38-27b");
        }
        command.arg("--json").env_clear().kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(8), command.output()).await??;
        anyhow::ensure!(
            output.status.success() && output.stdout.len() <= 262144,
            ERROR
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        anyhow::ensure!(
            value["mode"] == "vast-prototype" && value["ok"] == true,
            ERROR
        );
        Ok(value)
    }
    async fn owner(&self) -> anyhow::Result<(Owner, Value)> {
        let owner: Owner =
            serde_json::from_value(read_private(&self.attempt.join("desktop-owner.json"))?)?;
        anyhow::ensure!(
            owner.schema == "m8s.desktop-owner/v1"
                && owner.api_port != 0
                && owner.console_port != 0
                && owner.expires_at > chrono::Utc::now().timestamp(),
            ERROR
        );
        let status = self.get_json(owner.console_port, "/api/status").await?;
        anyhow::ensure!(verified_owner(&owner, &status), ERROR);
        Ok((owner, status))
    }
    async fn get_json(&self, port: u16, path: &str) -> anyhow::Result<Value> {
        let response = self
            .http
            .get(format!("http://127.0.0.1:{port}{path}"))
            .timeout(Duration::from_secs(3))
            .send()
            .await?
            .error_for_status()?;
        bounded_json(response).await
    }
    async fn snapshot(&self) -> anyhow::Result<Value> {
        let response = self.command("logs").await?;
        let events: Vec<&str> = response["events"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!(ERROR))?
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.strip_prefix("prototype stage: ").unwrap_or(s))
            .collect();
        let claimed = self
            .config
            .control_directory
            .join("launch-claimed")
            .exists();
        let down = self
            .config
            .control_directory
            .join("down-requested")
            .exists();
        let removed = events.contains(&"exact-instance-absence-confirmed")
            && read_private(&self.attempt.join("cleanup-evidence.json")).is_ok_and(|v| {
                v["schema"] == "m8s.vast-removal/v1"
                    && v["image"] == self.image
                    && v["creator_account_id"] == self.account
                    && v["provider_confirmed_absent"] == true
                    && v["instance_id"].as_u64().is_some_and(|id| id > 0)
            });
        let canary = events.contains(&"owner-routed-inference-completed");
        let failed = events.iter().any(|e| e.contains("coordinator failed"));
        let mut stage = if removed {
            "removed"
        } else if down {
            "removing"
        } else if failed {
            "uncertain"
        } else if claimed {
            "provisioning"
        } else {
            "idle"
        };
        let mut node = Value::Null;
        let mut expires_at = Value::Null;
        if !removed
            && !down
            && !failed
            && let Ok((owner, status)) = self.owner().await
        {
            // Exclude join tokens and unneeded native fields from the WebView.
            node = json!({"node_id":status["node_id"], "peers":status["peers"], "my_hostname":status["my_hostname"]});
            expires_at = json!(owner.expires_at);
            if events.contains(&"certificate-delivered-control-established") {
                stage = "loading";
            }
            if canary {
                let models = self.get_json(owner.api_port, "/v1/models").await?;
                if models["data"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|r| r["id"] == MODEL))
                {
                    stage = "ready";
                }
            }
        }
        Ok(
            json!({"enabled":true,"stage":stage,"cap_micro_usd":self.cap,"model":"Qwen 27B","node":node,"expires_at":expires_at}),
        )
    }
    async fn completion(&self, text: &str) -> anyhow::Result<String> {
        anyhow::ensure!(
            !text.trim().is_empty() && text.len() <= 2048,
            "Keep the demo prompt below 2048 bytes"
        );
        let mut history = self
            .history
            .try_lock()
            .map_err(|_| anyhow::anyhow!("A reply is already running"))?;
        anyhow::ensure!(
            history
                .iter()
                .filter_map(|v| v["content"].as_str())
                .map(str::len)
                .sum::<usize>()
                + text.len()
                <= 3000,
            "Demo conversation is full; finish this bounded session"
        );
        anyhow::ensure!(self.snapshot().await?["stage"] == "ready", ERROR);
        let (owner, _) = self.owner().await?;
        let mut messages = history.clone();
        messages.push(json!({"role":"user","content":text}));
        let body = json!({"model":MODEL,"messages":messages,"stream":false,"max_tokens":256,"temperature":0.3,"chat_template_kwargs":{"enable_thinking":false}});
        let remaining = (owner.expires_at - chrono::Utc::now().timestamp()).clamp(1, 60) as u64;
        let response = self
            .http
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                owner.api_port
            ))
            .timeout(Duration::from_secs(remaining))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let value = bounded_json(response).await?;
        let answer = checked_answer(&value)?;
        history.push(json!({"role":"user","content":text}));
        history.push(json!({"role":"assistant","content":answer}));
        Ok(answer.to_string())
    }
}
fn verified_owner(o: &Owner, v: &Value) -> bool {
    v["node_id"] == o.node_id
        && v["owner"]["owner_id"] == o.owner_id
        && v["owner"]["cert_id"] == o.cert_id
        && v["owner"]["verified"] == true
        && v["owner"]["status"] == "verified"
        && v["is_client"] == true
        && v["is_host"] == false
        && v["llama_ready"] == false
}
fn checked_answer(v: &Value) -> anyhow::Result<&str> {
    anyhow::ensure!(
        v["model"] == RESPONSE_MODEL && v["choices"][0]["finish_reason"] == "stop",
        "Response did not confirm the pinned Qwen model and a complete answer"
    );
    let text = v["choices"][0]["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 8192)
        .ok_or_else(|| anyhow::anyhow!(ERROR))?;
    Ok(text)
}
async fn bounded_json(mut response: reqwest::Response) -> anyhow::Result<Value> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(bytes.len() + chunk.len() <= 262144, ERROR);
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn failure() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error":ERROR})),
    )
        .into_response()
}
pub async fn status(State(state): State<Arc<AppState>>) -> Response {
    let Some(compute) = &state.compute else {
        return Json(json!({"enabled":false})).into_response();
    };
    match compute.snapshot().await {
        Ok(v) => Json(v).into_response(),
        Err(_) => Json(json!({"enabled":true,"stage":"uncertain","cap_micro_usd":compute.cap,"model":"Qwen 27B","node":null,"expires_at":null})).into_response(),
    }
}
async fn action(state: &AppState, headers: HeaderMap, verb: &str) -> Response {
    let authority = format!("127.0.0.1:{}", state.ports.app);
    if headers.get("host").and_then(|v| v.to_str().ok()) != Some(authority.as_str())
        || headers
            .get("origin")
            .is_some_and(|v| v.to_str().ok() != Some(format!("http://{authority}").as_str()))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    // Custom header requires same-origin JS or a CORS preflight; no CORS grant
    // is installed. Simple cross-origin forms cannot spend or stop a rental.
    if headers.get("x-m8s-desktop").and_then(|v| v.to_str().ok()) != Some("1") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(compute) = &state.compute else {
        return failure();
    };
    let _guard = compute.action.lock().await;
    if verb == "up"
        && compute
            .config
            .control_directory
            .join("launch-claimed")
            .exists()
    {
        return StatusCode::CONFLICT.into_response();
    }
    match compute.command(verb).await {
        Ok(_) => Json(json!({"accepted":true})).into_response(),
        Err(_) => failure(),
    }
}
pub async fn up(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    action(&state, headers, "up").await
}
pub async fn down(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    action(&state, headers, "down").await
}
pub async fn chat(compute: &Compute, text: &str) -> Response {
    use axum::response::sse::{Event, Sse};
    let frames = match compute.completion(text).await {
        Ok(answer) => vec![Event::default().event("response.output_text.delta").data(json!({"type":"response.output_text.delta","delta":answer}).to_string()), Event::default().event("response.completed").data(json!({"type":"response.completed","response":{"model":RESPONSE_MODEL}}).to_string()), Event::default().data("[DONE]")],
        Err(_) => vec![Event::default().event("error").data(json!({"type":"error","message":"Qwen reply unavailable or unverified. The rental deadline and cleanup remain active."}).to_string()), Event::default().data("[DONE]")],
    };
    Sse::new(futures_util::stream::iter(
        frames.into_iter().map(Ok::<_, std::convert::Infallible>),
    ))
    .into_response()
}
pub async fn history(compute: &Compute) -> Response {
    let history = compute.history.lock().await;
    Json(
        history
            .iter()
            .enumerate()
            .map(|(i, m)| json!({"id":format!("demo-{i}"),"role":m["role"],"text":m["content"]}))
            .collect::<Vec<_>>(),
    )
    .into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attachment_files_refuse_public_permissions_and_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = root.join("attachment.json");
        std::fs::write(&file, b"{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_private(&file).is_ok());
        symlink(&file, root.join("alias.json")).unwrap();
        assert!(read_private(&root.join("alias.json")).is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_private(&file).is_err());
    }
    #[tokio::test]
    async fn stale_canary_and_removal_labels_do_not_prove_live_readiness_or_absence() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cli = root.join("synthetic-cli");
        std::fs::write(&cli, b"#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"mode\":\"vast-prototype\",\"events\":[\"prototype stage: owner-routed-inference-completed\",\"prototype stage: exact-instance-absence-confirmed\"]}'\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(root.join("launch-claimed"), b"claimed").unwrap();
        let c = Compute {
            config: Config {
                cli,
                control_directory: root.clone(),
            },
            attempt: root.clone(),
            cap: 1_200_000,
            image: "synthetic-image".into(),
            account: 123,
            http: reqwest::Client::new(),
            action: Mutex::new(()),
            history: Mutex::new(vec![]),
        };
        assert_eq!(c.snapshot().await.unwrap()["stage"], "provisioning");
        std::fs::write(root.join("down-requested"), b"down").unwrap();
        assert_eq!(c.snapshot().await.unwrap()["stage"], "removing");
        let receipt = root.join("cleanup-evidence.json");
        std::fs::write(&receipt, serde_json::to_vec(&json!({"schema":"m8s.vast-removal/v1","image":"synthetic-image","creator_account_id":123,"instance_id":7,"provider_confirmed_absent":true})).unwrap()).unwrap();
        std::fs::set_permissions(&receipt, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(c.snapshot().await.unwrap()["stage"], "removed");
    }
    #[test]
    fn budget_preserves_baseline_and_refuses_missing_or_excess_allowance() {
        let mut p = json!({"schema":"m8s.vast-demo/v1","model":"qwen3.8-27b","tier":"qwen38-27b","policy":{"trial_all_in":8_000_000,"baseline":{"prior_lifetime_spent":3_000_000}}});
        assert_eq!(allowance(&p).unwrap(), 5_000_000);
        p["policy"]["trial_all_in"] = json!(8_000_001);
        assert!(allowance(&p).is_err());
        p["policy"]["trial_all_in"] = json!(2);
        assert!(allowance(&p).is_err());
        assert!(allowance(&json!({})).is_err());
    }
    #[test]
    fn responses_cannot_substitute_auto_or_incomplete_output() {
        let mut v = json!({"model":RESPONSE_MODEL,"choices":[{"finish_reason":"stop","message":{"content":"A mesh connects nodes."}}]});
        assert!(checked_answer(&v).is_ok());
        v["model"] = json!("auto");
        assert!(checked_answer(&v).is_err());
        v["model"] = json!(RESPONSE_MODEL);
        v["choices"][0]["finish_reason"] = json!("length");
        assert!(checked_answer(&v).is_err());
    }
    #[test]
    fn certificates_and_model_free_role_must_match() {
        let o = Owner {
            schema: String::new(),
            api_port: 1,
            console_port: 2,
            node_id: "node".into(),
            owner_id: "owner".into(),
            cert_id: "cert".into(),
            expires_at: 1,
        };
        let mut v = json!({"node_id":"node","owner":{"owner_id":"owner","cert_id":"cert","verified":true,"status":"verified"},"is_client":true,"is_host":false,"llama_ready":false});
        assert!(verified_owner(&o, &v));
        v["owner"]["cert_id"] = json!("other");
        assert!(!verified_owner(&o, &v));
        v["owner"]["cert_id"] = json!("cert");
        v["is_host"] = json!(true);
        assert!(!verified_owner(&o, &v));
    }
}
