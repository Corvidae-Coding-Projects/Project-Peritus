//! Current input fencing, message construction, and provider-terminal classification.

use crate::{DeveloperInteraction, DeveloperLoopError, ModelSession};
use peritus_model_protocol::{
    BoundedText, CanonicalJson, ContentBlock, JsonBounds, Message, ProtocolLimits, ReducedItem,
    Role, TerminalOutcome,
};

pub(super) fn input_changed(
    interaction: Option<&dyn DeveloperInteraction>,
    revision: u64,
) -> Result<bool, DeveloperLoopError> {
    interaction.map_or(Ok(false), |port| port.input().map(|input| input.revision != revision))
}

pub(super) struct UserResponse {
    pub(super) message: String,
    pub(super) continue_work: bool,
}

pub(super) fn user_response(
    text: &str,
    latest_user_text: Option<&str>,
) -> Result<UserResponse, DeveloperLoopError> {
    let host_wait = latest_user_text.is_some_and(explicit_wait_request);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(DeveloperLoopError::EmptyResponse);
    }
    // Accept the old private control object when replaying an in-flight provider turn.
    if let Ok(checked) =
        CanonicalJson::parse(trimmed, JsonBounds::value(ProtocolLimits::PRODUCTION))
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(checked.canonical_bytes())
        && let Some(object) = value.as_object().filter(|object| object.len() == 2)
        && let (Some(message), Some(continue_work)) = (
            object.get("message").and_then(serde_json::Value::as_str),
            object.get("continue_work").and_then(serde_json::Value::as_bool),
        )
        && !message.trim().is_empty()
    {
        return bounded_response(message.trim(), continue_work && !host_wait);
    }
    let (marker, body) = trimmed.split_once('\n').unwrap_or(("", trimmed));
    let (continue_work, message) = if marker.trim().eq_ignore_ascii_case("WAIT") {
        (false, body.trim())
    } else if marker.trim().eq_ignore_ascii_case("CONTINUE") {
        (true, body.trim())
    } else {
        (true, trimmed)
    };
    // A model may still replay an incomplete old control object. Its private syntax is never
    // shown to the user and cannot decide whether work resumes.
    let message =
        if message.is_empty() || (message.starts_with('{') && message.contains("continue_work")) {
            if host_wait || !continue_work {
                "I will wait for your next message."
            } else {
                "I received your message and will continue."
            }
        } else {
            message
        };
    bounded_response(message, continue_work && !host_wait)
}

fn bounded_response(
    message: &str,
    continue_work: bool,
) -> Result<UserResponse, DeveloperLoopError> {
    drop(BoundedText::new(message.to_owned(), ProtocolLimits::PRODUCTION)?);
    Ok(UserResponse { message: message.to_owned(), continue_work })
}

fn explicit_wait_request(input: &str) -> bool {
    let lower = input.trim().to_ascii_lowercase();
    matches!(lower.as_str(), "wait" | "pause" | "stop" | "cancel" | "hold on")
        || [
            "wait for ",
            "wait until ",
            "pause ",
            "stop ",
            "cancel ",
            "hold on ",
            "do not continue",
            "don't continue",
            "do not proceed",
            "don't proceed",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || lower.ends_with(" then wait")
}

pub(super) const fn successful(terminal: Option<&TerminalOutcome>) -> bool {
    matches!(
        terminal,
        Some(TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. })
    )
}

pub(super) fn usable(session: &ModelSession) -> bool {
    session.completed_items().iter().any(|item| match item {
        ReducedItem::Text { text, .. } => !text.expose_for_wire().trim().is_empty(),
        ReducedItem::ToolCall { .. } | ReducedItem::Refusal { .. } => true,
        ReducedItem::Reasoning { .. }
        | ReducedItem::Structured { .. }
        | ReducedItem::ProviderNative { .. } => false,
    })
}

pub(super) fn terminal_error(terminal: Option<&TerminalOutcome>) -> DeveloperLoopError {
    match terminal {
        Some(TerminalOutcome::Failed(failure)) => DeveloperLoopError::ProviderTerminal {
            provider: failure.provider().as_str().to_owned(),
            category: failure.category(),
            diagnostic_code: failure.diagnostic().code().to_owned(),
            http_status: failure.http_status(),
        },
        Some(TerminalOutcome::Refused { .. }) => DeveloperLoopError::Refused,
        Some(TerminalOutcome::Cancelled) => DeveloperLoopError::Cancelled,
        Some(
            TerminalOutcome::Succeeded { .. }
            | TerminalOutcome::RequiresAction { .. }
            | TerminalOutcome::Incomplete { .. },
        )
        | None => DeveloperLoopError::EmptyResponse,
    }
}

pub(super) fn message(
    role: Role,
    value: String,
    limits: ProtocolLimits,
) -> Result<Message, DeveloperLoopError> {
    Ok(Message::new(role, vec![ContentBlock::Text(BoundedText::new(value, limits)?)], limits)?)
}

pub(super) fn user_message(
    value: String,
    attachments: Vec<peritus_model_protocol::MediaInput>,
    limits: ProtocolLimits,
) -> Result<Message, DeveloperLoopError> {
    let mut content = Vec::with_capacity(attachments.len().saturating_add(1));
    content.push(ContentBlock::Text(BoundedText::new(value, limits)?));
    content.extend(attachments.into_iter().map(ContentBlock::Image));
    Ok(Message::new(Role::User, content, limits)?)
}

/// Converts completed provider items into the exact next assistant transcript.
pub(super) fn assistant_items(
    items: &[ReducedItem],
    protocol_limits: ProtocolLimits,
) -> Result<
    (Vec<ContentBlock>, Vec<peritus_model_protocol::CompletedToolCall>, String),
    DeveloperLoopError,
> {
    let mut assistant = Vec::new();
    let mut calls = Vec::new();
    let mut final_text = String::new();
    for item in items {
        match item {
            ReducedItem::Text { text, .. } => {
                final_text.push_str(text.expose_for_wire());
                assistant.push(ContentBlock::Text(text.clone()));
            }
            ReducedItem::ToolCall { call, .. } => {
                calls.push(call.clone());
                assistant.push(ContentBlock::ToolCall(call.clone()));
            }
            ReducedItem::Refusal { .. } => return Err(DeveloperLoopError::Refused),
            // A visible summary without opaque state is not a replayable wire item.
            ReducedItem::Reasoning { replay, .. } if replay.is_empty() => {}
            ReducedItem::Reasoning { summary, replay, .. } => {
                assistant.push(ContentBlock::Reasoning(
                    peritus_model_protocol::ReasoningReplay::new(
                        summary.clone(),
                        replay.clone(),
                        protocol_limits,
                    )?,
                ));
            }
            ReducedItem::Structured { .. } | ReducedItem::ProviderNative { .. } => {}
        }
    }

    Ok((assistant, calls, final_text))
}
