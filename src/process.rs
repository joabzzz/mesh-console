use std::process::{Command, Stdio};

const BINARY: &str = "mesh-llm";

/// Spawn `mesh-llm serve --model <model> [--mesh-name <name>] [--publish]`.
/// The process is detached — it keeps running after we return.
pub fn spawn_serve(model: &str, mesh_name: Option<&str>, is_public: bool) -> Result<(), String> {
    let mut cmd = Command::new(BINARY);
    cmd.arg("serve").arg("--model").arg(model);
    if let Some(name) = mesh_name.filter(|n| !n.is_empty()) {
        cmd.arg("--mesh-name").arg(name);
    }
    if is_public {
        cmd.arg("--publish");
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn()
        .map(|_| ()) // drop handle — process runs as daemon
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "mesh-llm not found. Install it first.".to_string()
            } else {
                format!("failed to start mesh-llm: {e}")
            }
        })
}

/// Spawn `mesh-llm serve --join <token>`.
/// The process is detached — it keeps running after we return.
pub fn spawn_join(token: &str) -> Result<(), String> {
    Command::new(BINARY)
        .arg("serve")
        .arg("--join")
        .arg(token)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "mesh-llm not found. Install it first.".to_string()
            } else {
                format!("failed to start mesh-llm: {e}")
            }
        })
}

/// Send SIGTERM (Unix) or terminate (Windows) to a specific mesh-llm pid.
///
/// Prefer this over [`stop_daemon`] when the management API exposes a
/// `local_instances[].pid` so we don't accidentally kill an unrelated mesh
/// daemon running on the same machine.
pub fn stop_pid(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .map_err(|e| format!("kill failed: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("kill exited with status {status}"))
        }
    }
    #[cfg(not(unix))]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status()
            .map(|_| ())
            .map_err(|e| format!("taskkill failed: {e}"))
    }
}

/// Stop any running mesh-llm daemon on this machine.
#[allow(dead_code)]
pub fn stop_daemon() -> Result<(), String> {
    // Try SIGTERM first via pkill, then check the result
    #[cfg(unix)]
    {
        let status = Command::new("pkill")
            .arg("-x")
            .arg(BINARY)
            .status()
            .map_err(|e| format!("pkill failed: {e}"))?;
        // pkill exits 0 if it killed something, 1 if nothing found
        if status.success() || status.code() == Some(1) {
            Ok(())
        } else {
            Err(format!("pkill exited with status {status}"))
        }
    }
    #[cfg(not(unix))]
    {
        Command::new("taskkill")
            .args(["/IM", &format!("{BINARY}.exe"), "/F"])
            .status()
            .map(|_| ())
            .map_err(|e| format!("taskkill failed: {e}"))
    }
}
