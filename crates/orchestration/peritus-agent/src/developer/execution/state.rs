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
