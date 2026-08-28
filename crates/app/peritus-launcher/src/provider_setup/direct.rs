//! Hidden credential entry and direct-provider settings prompts.

use std::io::{self, Write as _};

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use peritus_product_state::{CompatibleProtocol, DirectProviderProfile, ProviderKind};
use peritus_provider_onboarding::{DirectCredential, DirectProviderDraft};
use zeroize::Zeroizing;

use super::Terminal;
use crate::LauncherError;

const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;

pub(super) fn setup(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
) -> Result<DirectProviderProfile, LauncherError> {
    terminal.line("")?;
    terminal.line(kind.label())?;
    terminal.line("The key will be stored by your operating system, not in Peritus files.")?;

    let (endpoint, model, protocol, header) = settings(terminal, kind)?;
    terminal.line("Paste the API key and press Enter. Input is hidden: ")?;
    let credential = read_secret()?;
    terminal.line("Credential captured. Saving it to the operating-system credential store…")?;
    let draft = DirectProviderDraft::new(kind, endpoint, model, protocol, header);
    let profile = draft.store(&credential)?;
    terminal.line(&format!("{} is configured.", kind.label()))?;
    Ok(profile)
}

fn settings(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
) -> Result<DirectSettings, LauncherError> {
    match kind {
        ProviderKind::OpenAiApi => {
            Ok((None, defaulted(terminal, "Model [gpt-5.6-sol]: ", "gpt-5.6-sol")?, None, None))
        }
        ProviderKind::AnthropicApi => Ok((
            Some("https://api.anthropic.com".to_owned()),
            defaulted(terminal, "Model [claude-sonnet-4-5]: ", "claude-sonnet-4-5")?,
            None,
            None,
        )),
        ProviderKind::GoogleGeminiApi => Ok((
            Some("https://generativelanguage.googleapis.com".to_owned()),
            defaulted(terminal, "Model [gemini-3.7-flash]: ", "gemini-3.7-flash")?,
            None,
            None,
        )),
        ProviderKind::CompatibleEndpoint => compatible_settings(terminal),
        _ => Err(LauncherError::Interaction(
            "the selected provider does not use direct credential setup".to_owned(),
        )),
    }
}

type DirectSettings = (Option<String>, String, Option<CompatibleProtocol>, Option<String>);

fn compatible_settings(terminal: &mut Terminal<'_>) -> Result<DirectSettings, LauncherError> {
    let endpoint = required(terminal, "Endpoint URL: ")?;
    let model = required(terminal, "Model name: ")?;
    terminal.line("Protocol: 1. Responses (default)  2. Chat Completions")?;
    let protocol = loop {
        match terminal.prompt("Protocol [1]: ")?.as_str() {
            "" | "1" => break CompatibleProtocol::Responses,
            "2" => break CompatibleProtocol::ChatCompletions,
            _ => terminal.line("Choose 1 or 2.")?,
        }
    };
    let header = terminal.prompt(
        "Credential header [Enter for Authorization: Bearer, or type an API-key header]: ",
    )?;
    Ok((Some(endpoint), model, Some(protocol), (!header.is_empty()).then_some(header)))
}

fn defaulted(
    terminal: &mut Terminal<'_>,
    prompt: &str,
    default: &str,
) -> Result<String, LauncherError> {
    let answer = terminal.prompt(prompt)?;
    Ok(if answer.is_empty() { default.to_owned() } else { answer })
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

fn read_secret() -> Result<DirectCredential, LauncherError> {
    let _guard = RawInputGuard::enter()?;
    let mut bytes = Zeroizing::new(Vec::new());
    loop {
        match event::read().map_err(|error| interaction(&error))? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c' | 'C'))
                {
                    return Err(LauncherError::Interaction(
                        "credential entry cancelled; run `peritus` to resume".to_owned(),
                    ));
                }
                match key.code {
                    KeyCode::Enter => break,
                    KeyCode::Backspace => pop_character(&mut bytes),
                    KeyCode::Char(value)
                        if !value.is_control()
                            && !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
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
    writeln!(io::stdout()).map_err(|error| interaction(&error))?;
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

struct RawInputGuard;

impl RawInputGuard {
    fn enter() -> Result<Self, LauncherError> {
        enable_raw_mode().map_err(|error| interaction(&error))?;
        if let Err(error) = execute!(io::stdout(), EnableBracketedPaste) {
            let _ignored = disable_raw_mode();
            return Err(interaction(&error));
        }
        io::stdout().flush().map_err(|error| interaction(&error))?;
        Ok(Self)
    }
}

impl Drop for RawInputGuard {
    fn drop(&mut self) {
        let _ignored = execute!(io::stdout(), DisableBracketedPaste);
        let _ignored = disable_raw_mode();
    }
}

fn interaction(error: &io::Error) -> LauncherError {
    LauncherError::Interaction(error.to_string())
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
}
