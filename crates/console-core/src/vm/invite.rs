//! Invite/copy command strings, matching the web console's AppHeader
//! (`mesh-llm-ui/src/features/app-shell/components/AppHeader.tsx`).

/// Join-and-serve command (recommended "contribute compute" invite).
pub fn invite_with_model_command(token: &str, model: Option<&str>) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    match model.filter(|m| !m.is_empty()) {
        Some(model) => Some(format!("mesh-llm --join {token} --model {model}")),
        None => Some(format!("mesh-llm --join {token}")),
    }
}

/// API-only client join command.
pub fn invite_client_command(token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    Some(format!("mesh-llm --client --join {token}"))
}

/// OpenAI-compatible endpoint URL for external apps.
pub fn api_direct_url(api_port: u16) -> Option<String> {
    if api_port == 0 {
        return None;
    }
    Some(format!("http://127.0.0.1:{api_port}/v1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_match_web_console_format() {
        assert_eq!(
            invite_with_model_command("tok", Some("GLM-4.7-Flash-Q4_K_M")).as_deref(),
            Some("mesh-llm --join tok --model GLM-4.7-Flash-Q4_K_M")
        );
        assert_eq!(
            invite_client_command("tok").as_deref(),
            Some("mesh-llm --client --join tok")
        );
        assert_eq!(
            api_direct_url(9337).as_deref(),
            Some("http://127.0.0.1:9337/v1")
        );
    }

    #[test]
    fn empty_inputs_yield_none() {
        assert_eq!(invite_with_model_command("", Some("m")), None);
        assert_eq!(invite_client_command(""), None);
        assert_eq!(api_direct_url(0), None);
        assert_eq!(
            invite_with_model_command("tok", None).as_deref(),
            Some("mesh-llm --join tok")
        );
    }
}
