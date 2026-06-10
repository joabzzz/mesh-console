//! `POST /api/responses` streaming chat — the same endpoint and event grammar
//! the web console uses, going through the management port (so mesh routing,
//! "auto" model selection, and served-by attribution all apply).

use crate::state::{ChatSseEvent, CompletedResponse, ResponsesRequest};
use futures::StreamExt;

use super::{sse, ApiClient};

/// Digested stream items for the chat UI. Server event types we don't render
/// (created/done/content_part…) are filtered out.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatStreamItem {
    TextDelta(String),
    ReasoningDelta(String),
    Completed(CompletedResponse),
}

impl ApiClient {
    /// Run one streaming responses request, forwarding digested items through
    /// `tx`. Returns when the server closes the stream (normally right after
    /// `response.completed`) or with an error describing the failure.
    pub async fn stream_responses(
        &self,
        request: &ResponsesRequest,
        tx: tokio::sync::mpsc::UnboundedSender<ChatStreamItem>,
    ) -> Result<(), String> {
        let resp = self
            .http()
            .post(self.url("/api/responses"))
            .json(request)
            .send()
            .await
            .map_err(|e| format!("responses connect failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("responses HTTP {status}: {body}"));
        }

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut completed = false;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("responses read failed: {e}"))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            for payload in sse::drain_data_payloads(&mut buf) {
                let Ok(event) = serde_json::from_str::<ChatSseEvent>(&payload) else {
                    continue;
                };
                let item = match event {
                    ChatSseEvent::OutputTextDelta { delta, .. } => ChatStreamItem::TextDelta(delta),
                    ChatSseEvent::ReasoningTextDelta { delta, .. } => {
                        ChatStreamItem::ReasoningDelta(delta)
                    }
                    ChatSseEvent::Completed { response } => {
                        completed = true;
                        ChatStreamItem::Completed(response)
                    }
                    ChatSseEvent::Other(_) => continue,
                };
                if tx.send(item).is_err() {
                    return Ok(());
                }
            }
            if completed {
                break;
            }
        }

        if completed {
            Ok(())
        } else {
            Err("responses stream closed before completion".to_string())
        }
    }
}
