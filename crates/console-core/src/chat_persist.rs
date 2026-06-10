//! Local JSON persistence for mesh-console chat sessions (desktop data dir).
//! The format is versioned by leniency: every field is `#[serde(default)]`
//! so archives written by older console builds keep loading.

use crate::state::{ChatTimings, ChatUsage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_DIR: &str = "mesh-console";
const CHATS_FILE: &str = "chats.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

/// Attachment metadata kept with a message (bytes live on the daemon).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AttachmentMeta {
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatLine {
    pub role: ChatRole,
    pub content: String,
    /// Model reasoning stream (when the model emits one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ChatUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<ChatTimings>,
    /// Which peer served the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub served_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentMeta>,
}

impl ChatLine {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            reasoning: None,
            usage: None,
            timings: None,
            served_by: None,
            attachments: Vec::new(),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatSessionRecord {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub lines: Vec<ChatLine>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatArchive {
    pub sessions: Vec<ChatSessionRecord>,
    pub active_id: String,
}

fn storage_path() -> Result<PathBuf, String> {
    let base =
        dirs::data_local_dir().ok_or_else(|| "could not resolve data_local_dir".to_string())?;
    Ok(base.join(APP_DIR).join(CHATS_FILE))
}

pub fn new_session_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("s-{ms}")
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn session_title_from_message(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "New chat".to_string();
    }
    let mut s: String = t.chars().take(48).collect();
    if t.chars().count() > 48 {
        s.push('…');
    }
    s
}

impl ChatArchive {
    pub fn fresh() -> Self {
        let id = new_session_id();
        Self {
            active_id: id.clone(),
            sessions: vec![ChatSessionRecord {
                id,
                title: "New chat".to_string(),
                updated_at: now_unix(),
                lines: Vec::new(),
            }],
        }
    }

    pub fn active_session_mut(&mut self) -> Option<&mut ChatSessionRecord> {
        let id = self.active_id.clone();
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn touch_active_title_from_user_message(&mut self, user_text: &str) {
        if let Some(sess) = self.active_session_mut() {
            sess.updated_at = now_unix();
            if sess.title == "New chat"
                || sess
                    .lines
                    .iter()
                    .filter(|l| l.role == ChatRole::User)
                    .count()
                    <= 1
            {
                sess.title = session_title_from_message(user_text);
            }
        }
    }

    pub fn sort_sessions_for_sidebar(&self) -> Vec<&ChatSessionRecord> {
        let mut v: Vec<_> = self.sessions.iter().collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        v
    }
}

pub fn load_archive() -> ChatArchive {
    let Ok(path) = storage_path() else {
        return ChatArchive::fresh();
    };
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(mut arch) = serde_json::from_slice::<ChatArchive>(&bytes) {
            if arch.sessions.is_empty() {
                return ChatArchive::fresh();
            }
            if !arch.sessions.iter().any(|s| s.id == arch.active_id) {
                arch.active_id = arch.sessions[0].id.clone();
            }
            return arch;
        }
    }
    ChatArchive::fresh()
}

pub fn save_archive(arch: &ChatArchive) -> Result<(), String> {
    let path = storage_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_vec_pretty(arch).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_archive_format_still_loads() {
        // Pre-M3 archives have only role + content on lines.
        let old = r#"{
            "active_id": "s-1",
            "sessions": [{
                "id": "s-1", "title": "t", "updated_at": 1,
                "lines": [{"role": "user", "content": "hi"}]
            }]
        }"#;
        let arch: ChatArchive = serde_json::from_str(old).expect("lenient load");
        let line = &arch.sessions[0].lines[0];
        assert_eq!(line.content, "hi");
        assert!(line.reasoning.is_none() && line.usage.is_none());
        assert!(line.attachments.is_empty());
    }

    #[test]
    fn round_trip_preserves_metadata() {
        let mut line = ChatLine::new(ChatRole::Assistant, "answer");
        line.reasoning = Some("thought".into());
        line.served_by = Some("studio".into());
        let json = serde_json::to_string(&line).unwrap();
        let back: ChatLine = serde_json::from_str(&json).unwrap();
        assert_eq!(back, line);
    }
}
