//! Current input fencing, message construction, and provider-terminal classification.

use crate::{DeveloperInteraction, DeveloperLoopError, ModelSession};
use peritus_model_protocol::{
    BoundedText, ContentBlock, Message, ProtocolLimits, ReducedItem, Role, TerminalOutcome,
};

pub(super) fn input_changed(
    interaction: Option<&dyn DeveloperInteraction>,
    revision: u64,
) -> Result<bool, DeveloperLoopError> {
    interaction.map_or(Ok(false), |port| port.input().map(|input| input.revision != revision))
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
