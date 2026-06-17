//! `POST /api/objects` — attachment uploads for chat (images, PDFs, audio).
//! The returned token is referenced from a `ResponsesContentBlock` in the
//! next `/api/responses` request, exactly like the web console's composer.

use crate::state::{AttachmentUploadRequest, AttachmentUploadResponse};
use base64::Engine;

use super::ApiClient;

/// Picked file ready for upload.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentFile {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// Infer a MIME type from the file extension; the daemon needs one to decide
/// image/audio/file handling.
pub fn mime_type_for(file_name: &str) -> String {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "txt" | "md" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

impl ApiClient {
    /// Upload one attachment; returns the object token to reference from the
    /// chat request.
    pub async fn upload_attachment(
        &self,
        request_id: &str,
        file: &AttachmentFile,
    ) -> Result<String, String> {
        let body = AttachmentUploadRequest {
            request_id: request_id.to_string(),
            mime_type: file.mime_type.clone(),
            file_name: file.file_name.clone(),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(&file.bytes),
        };
        let resp = self
            .http()
            .post(self.url("/api/objects"))
            .timeout(std::time::Duration::from_secs(60))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("attachment upload failed: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("attachment upload HTTP {status}: {text}"));
        }
        let parsed: AttachmentUploadResponse = resp
            .json()
            .await
            .map_err(|e| format!("attachment upload parse error: {e}"))?;
        if parsed.token.is_empty() {
            return Err("attachment upload returned no token".to_string());
        }
        Ok(parsed.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types_cover_console_attachment_kinds() {
        assert_eq!(mime_type_for("photo.JPG"), "image/jpeg");
        assert_eq!(mime_type_for("doc.pdf"), "application/pdf");
        assert_eq!(mime_type_for("note.wav"), "audio/wav");
        assert_eq!(mime_type_for("data.bin"), "application/octet-stream");
        assert_eq!(mime_type_for("noext"), "application/octet-stream");
    }
}
