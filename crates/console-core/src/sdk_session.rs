//! SDK-backed identity and invite helpers (from [`mesh_llm_sdk`], pinned in
//! the workspace `Cargo.toml`).
//!
//! Chat streams through the management API (`/api/responses`, see
//! `api::responses`) like the web console; this module owns what the SDK is
//! uniquely good for — parsing invite tokens and the persistent owner
//! keypair that both the chat client id and the embedded node's keystore
//! derive from.

use std::path::PathBuf;
use std::str::FromStr;

use mesh_llm_sdk::{InviteToken, OwnerKeypair};

/// Validate an invite token using the SDK's parser. Returns the canonical
/// string form on success and a human-readable error on failure. Join
/// buttons use this to reject malformed tokens before booting the embedded
/// node.
pub fn validate_invite_token(raw: &str) -> Result<String, String> {
    let token = InviteToken::from_str(raw.trim()).map_err(|e| format!("invalid invite: {e}"))?;
    Ok(token.as_str().to_string())
}

/// Stable owner id derived from the persisted keypair. Useful as a "this
/// device" identifier in the Home view; mirrors what mesh-llm peers see.
pub fn owner_id() -> Result<String, String> {
    Ok(load_or_create_owner_keypair()?.owner_id())
}

const APP_DIR: &str = "mesh-console";
const KEYPAIR_FILE: &str = "owner.hex";

fn keypair_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|d| d.join(APP_DIR).join(KEYPAIR_FILE))
        .ok_or_else(|| "could not resolve user config directory".to_string())
}

/// Load the persisted owner keypair, or generate and save one on first run.
/// The file is plain hex (signing || encryption) at
/// `<config_dir>/mesh-console/owner.hex` so the same identity is reused across
/// app launches; mesh-llm peers see a stable owner_id.
pub fn load_or_create_owner_keypair() -> Result<OwnerKeypair, String> {
    let path = keypair_path()?;
    if let Ok(text) = std::fs::read_to_string(&path) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return OwnerKeypair::from_hex(trimmed);
        }
    }
    let kp = OwnerKeypair::generate();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create config dir {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, kp.to_hex()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(kp)
}
