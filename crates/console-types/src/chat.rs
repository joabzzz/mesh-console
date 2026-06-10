//! `POST /api/responses` request body and SSE stream events, plus
//! `POST /api/objects` attachment uploads. Mirrors the chat types in
//! `mesh-llm-ui/src/lib/api/types.ts`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponsesContentBlock {
    #[serde(rename = "input_text")]
    Text { text: String },
    #[serde(rename = "input_image")]
    Image { image_url: String },
    #[serde(rename = "input_audio")]
    Audio { audio_url: String },
    #[serde(rename = "input_file")]
    File {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_name: Option<String>,
    },
}

/// Message content is either a plain string or a list of typed blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesContent {
    Text(String),
    Blocks(Vec<ResponsesContentBlock>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsesInputMessage {
    /// "system" | "user" | "assistant"
    pub role: String,
    pub content: ResponsesContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponsesRequest {
    /// Model name, or "auto" for mesh routing.
    pub model: String,
    pub client_id: String,
    pub request_id: String,
    pub input: Vec<ResponsesInputMessage>,
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatTimings {
    #[serde(default)]
    pub ttft_ms: Option<f64>,
    #[serde(default)]
    pub decode_time_ms: Option<f64>,
    #[serde(default)]
    pub total_time_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompletedResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: ChatUsage,
    #[serde(default)]
    pub timings: Option<ChatTimings>,
    #[serde(default)]
    pub served_by: Option<String>,
}

/// One SSE event on the `/api/responses` stream. Unknown event types map to
/// `Other` so new server events never break the stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatSseEvent {
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        delta: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
    },
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta {
        delta: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
    },
    #[serde(rename = "response.completed")]
    Completed { response: CompletedResponse },
    #[serde(untagged)]
    Other(serde_json::Value),
}

// ── Attachments (`/api/objects`) ─────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttachmentUploadRequest {
    pub request_id: String,
    pub mime_type: String,
    pub file_name: String,
    pub bytes_base64: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AttachmentUploadResponse {
    #[serde(default)]
    pub token: String,
}
