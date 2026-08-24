//! Ordered role/content messages with role-specific structural validation.

use crate::{ContentBlock, ProtocolError, ProtocolErrorKind, ProtocolLimits};

/// Provider-neutral conversational role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// System-level instruction.
    System,
    /// Developer instruction distinct from system/user input.
    Developer,
    /// End-user input.
    User,
    /// Prior model output or replay state.
    Assistant,
    /// Application tool result.
    Tool,
}

/// One nonempty ordered message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    role: Role,
    content: Vec<ContentBlock>,
}

impl Message {
    /// Creates and validates one role/content message.
    ///
    /// # Errors
    ///
    /// Rejects empty/oversized messages and content that is invalid for the selected role.
    pub fn new(
        role: Role,
        content: Vec<ContentBlock>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        if content.is_empty() || content.len() > limits.max_content_blocks() {
            return Err(invalid("message content is empty or exceeds its block bound"));
        }
        if content.iter().any(|block| !role_allows(role, block)) {
            return Err(invalid("message role does not permit one of its content blocks"));
        }
        Ok(Self { role, content })
    }

    /// Returns the role.
    #[must_use]
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Borrows ordered content.
    #[must_use]
    pub fn content(&self) -> &[ContentBlock] {
        &self.content
    }
}

const fn role_allows(role: Role, block: &ContentBlock) -> bool {
    match role {
        Role::System | Role::Developer => {
            matches!(block, ContentBlock::Text(_) | ContentBlock::ProviderExtension(_))
        }
        Role::User => matches!(
            block,
            ContentBlock::Text(_)
                | ContentBlock::Image(_)
                | ContentBlock::Audio(_)
                | ContentBlock::Document(_)
                | ContentBlock::ProviderExtension(_)
        ),
        Role::Assistant => matches!(
            block,
            ContentBlock::Text(_)
                | ContentBlock::ToolCall(_)
                | ContentBlock::Refusal(_)
                | ContentBlock::Reasoning(_)
                | ContentBlock::ProviderExtension(_)
        ),
        Role::Tool => {
            matches!(block, ContentBlock::ToolResult(_) | ContentBlock::ProviderExtension(_))
        }
    }
}

fn invalid(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidContent, "message", detail)
}
