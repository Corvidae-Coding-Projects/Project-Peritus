//! Hidden credential entry and direct-provider settings prompts.

use crossterm::{
    event::{Event, KeyCode, KeyEventKind, KeyModifiers},
};
use peritus_product_state::{CompatibleProtocol, DirectProviderProfile, ProviderKind};
use peritus_provider_core::CancellationToken;
use peritus_provider_onboarding::{
    DirectCredential, DirectProviderDraft, PreparedDirectProvider, ProviderEffectStore,
};
use zeroize::Zeroizing;

use crate::{
    LauncherError,
    terminal::{Terminal, input_cancelled},
};

const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;

pub(super) async fn setup(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
    effects: &ProviderEffectStore,
    cancellation: &CancellationToken,
) -> Result<PreparedRoute, LauncherError> {
    terminal.line("")?;
    terminal.line(kind.label())?;
    terminal.line("The key will be stored by your operating system, not in Peritus files.")?;

    let (endpoint, catalog_endpoint, model, protocol, header) = settings(terminal, kind)?;
    terminal.line("Paste the API key and press Enter. Input is hidden: ")?;
    let credential = read_secret(terminal)?;
    terminal.line("Credential captured. It will be published after setup is durably saved.")?;
    let draft = DirectProviderDraft::new(kind, endpoint, model, protocol, header);
    let draft = if let Some(endpoint) = catalog_endpoint {
        draft.with_catalog_endpoint(endpoint)
    } else {
        draft
    };
    terminal.line("Discovering available models from this provider…")?;
    let discovered = super::models::interruptible(
        cancellation,
        draft.discover_models(&credential, cancellation),
    )
    .await;
    let selection = super::models::choose_direct(terminal, kind, discovered)?;
    let draft = draft
        .with_model(selection.model)
        .with_model_facts(selection.facts);
    let draft = if let Some(protocol) = selection.protocol {
        draft.with_protocol(protocol)
    } else {
        draft
    };
    let publication = draft.prepare(credential, effects)?;
    terminal.line(&format!(
        "{} is ready to save. Connection not yet tested.",
        kind.label()
    ))?;
    Ok(PreparedRoute::new(publication))
}

pub(super) struct PreparedRoute {
    profile: DirectProviderProfile,
    publication: Option<PreparedDirectProvider>,
}

impl PreparedRoute {
    fn new(publication: PreparedDirectProvider) -> Self {
        Self { profile: publication.profile().clone(), publication: Some(publication) }
    }

    pub(super) fn retained(profile: DirectProviderProfile) -> Self {
        Self { profile, publication: None }
    }

    pub(super) const fn profile(&self) -> &DirectProviderProfile {
        &self.profile
    }

    pub(super) async fn publish(
        self,
        terminal: &mut Terminal<'_>,
        effects: &ProviderEffectStore,
        cancellation: &CancellationToken,
    ) -> Result<(), LauncherError> {
        let Some(publication) = self.publication else { return Ok(()) };
        let profile = publication.publish(effects)?;
        terminal.line(&format!("{} is configured.", profile.kind().label()))?;
        super::connection::offer(terminal, &profile, cancellation).await
    }
}

fn settings(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
) -> Result<DirectSettings, LauncherError> {
    match kind {
        ProviderKind::OpenAiApi => Ok((None, None, String::new(), None, None)),
        ProviderKind::AnthropicApi => {
            Ok((
                Some("https://api.anthropic.com".to_owned()),
                None,
                String::new(),
                None,
                None,
            ))
        }
        ProviderKind::GoogleGeminiApi => Ok((
            Some("https://generativelanguage.googleapis.com".to_owned()),
            None,
            String::new(),
            None,
            None,
        )),
        ProviderKind::CompatibleEndpoint => compatible_settings(terminal),
        _ if kind.hosted_service().is_some() => {
            Ok((None, None, String::new(), None, None))
        }
        _ => Err(LauncherError::Interaction(
            "the selected provider does not use direct credential setup".to_owned(),
        )),
    }
}

type DirectSettings = (
    Option<String>,
    Option<String>,
    String,
    Option<CompatibleProtocol>,
    Option<String>,
);

fn compatible_settings(terminal: &mut Terminal<'_>) -> Result<DirectSettings, LauncherError> {
    terminal.line("Protocol: 1. Responses  2. Chat Completions")?;
    let protocol = loop {
        match terminal.prompt("Protocol: ")?.as_str() {
            "1" => break CompatibleProtocol::Responses,
            "2" => break CompatibleProtocol::ChatCompletions,
            _ => terminal.line("Choose 1 or 2.")?,
        }
    };
    let endpoint = required(terminal, compatible_endpoint_prompt(protocol))?;
    let catalog_endpoint = terminal.prompt(
        "Exact model catalog URL [Enter to derive a standard same-origin /models route or use a manual model ID]: ",
    )?;
    let header = terminal.prompt(
        "Credential header [Enter for Authorization: Bearer, or type an API-key header]: ",
    )?;
    Ok((
        Some(endpoint),
        (!catalog_endpoint.is_empty()).then_some(catalog_endpoint),
        String::new(),
        Some(protocol),
        (!header.is_empty()).then_some(header),
    ))
}

const fn compatible_endpoint_prompt(protocol: CompatibleProtocol) -> &'static str {
    match protocol {
        CompatibleProtocol::Responses => "Exact endpoint URL (usually ending in /v1/responses): ",
        CompatibleProtocol::ChatCompletions => {
            "Exact endpoint URL (usually ending in /v1/chat/completions): "
        }
        CompatibleProtocol::AnthropicMessages | CompatibleProtocol::GoogleGenerateContent => {
            "Exact endpoint URL: "
        }
    }
}

fn required(terminal: &mut Terminal<'_>, prompt: &str) -> Result<String, LauncherError> {
    loop {
        let answer = terminal.prompt(prompt)?;
        if !answer.is_empty() {
            return Ok(answer);
        }
        terminal.line("This field is required.")?;
    }
}

fn read_secret(terminal: &mut Terminal<'_>) -> Result<DirectCredential, LauncherError> {
    let mut bytes = terminal.hidden_input(|input| {
        let mut bytes = Zeroizing::new(Vec::new());
        loop {
            match input.read_event()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(key.code, KeyCode::Char('c' | 'C'))
                    {
                        return Err(input_cancelled("credential entry"));
                    }
                    match key.code {
                        KeyCode::Enter => break,
                        KeyCode::Backspace => pop_character(&mut bytes),
                        KeyCode::Char(value)
                            if !value.is_control()
                                && !key.modifiers.intersects(
                                    KeyModifiers::CONTROL
                                        | KeyModifiers::ALT
                                        | KeyModifiers::SUPER,
                                ) =>
                        {
                            push_character(&mut bytes, value)?;
                        }
                        _ => {}
                    }
                }
                Event::Paste(value) => push_paste(&mut bytes, &value)?,
                _ => {}
            }
        }
        Ok(bytes)
    })?;
    terminal.line("")?;
    let owned = std::mem::take(&mut *bytes);
    DirectCredential::new(owned).map_err(LauncherError::Provider)
}

fn push_character(bytes: &mut Vec<u8>, value: char) -> Result<(), LauncherError> {
    let mut encoded = [0_u8; 4];
    let value = value.encode_utf8(&mut encoded).as_bytes();
    ensure_capacity(bytes.len(), value.len())?;
    bytes.extend_from_slice(value);
    Ok(())
}

fn push_paste(bytes: &mut Vec<u8>, value: &str) -> Result<(), LauncherError> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.chars().any(char::is_control) {
        return Err(LauncherError::Interaction(
            "credential paste contains unsupported control characters".to_owned(),
        ));
    }
    ensure_capacity(bytes.len(), value.len())?;
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn pop_character(bytes: &mut Vec<u8>) {
    if let Ok(value) = std::str::from_utf8(bytes)
        && let Some((index, _)) = value.char_indices().next_back()
    {
        bytes.truncate(index);
    }
}

fn ensure_capacity(current: usize, additional: usize) -> Result<(), LauncherError> {
    if current.saturating_add(additional) > MAX_CREDENTIAL_BYTES {
        return Err(LauncherError::Interaction(
            "credential input exceeds the supported size".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_newline_is_removed_but_embedded_controls_are_rejected() {
        let mut bytes = Vec::new();
        push_paste(&mut bytes, "provider-key\r\n").expect("paste");
        assert_eq!(bytes, b"provider-key");
        assert!(push_paste(&mut bytes, "bad\nkey").is_err());
    }

    #[test]
    fn backspace_removes_one_unicode_scalar() {
        let mut bytes = "key-🦀".as_bytes().to_vec();
        pop_character(&mut bytes);
        assert_eq!(bytes, b"key-");
    }

    #[test]
    fn compatible_endpoint_prompt_names_the_selected_operation_route() {
        assert!(
            compatible_endpoint_prompt(CompatibleProtocol::ChatCompletions)
                .contains("/v1/chat/completions")
        );
        assert!(
            compatible_endpoint_prompt(CompatibleProtocol::Responses).contains("/v1/responses")
        );
    }
}
