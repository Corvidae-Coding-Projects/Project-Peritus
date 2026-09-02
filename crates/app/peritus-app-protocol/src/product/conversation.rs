//! Bounded follow-up and transcript messages for product runs.

use peritus_types::RunId;

use super::{ProductRunMessageError, bounded_text};

/// Maximum UTF-8 bytes accepted for one conversational message.
pub const MAX_PRODUCT_MESSAGE_BYTES: usize = 64 * 1024;
/// Maximum messages retained in one product-run conversation.
pub const MAX_PRODUCT_MESSAGES: usize = 256;

/// Speaker of one persisted product-run conversation message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductConversationRole {
    /// A task, answer, correction, or follow-up supplied by the user.
    User,
    /// A question, result, or actionable failure reported by Peritus.
    Agent,
}

impl ProductConversationRole {
    /// Stable wire and persistence tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::User => 1,
            Self::Agent => 2,
        }
    }

    /// Decodes a stable wire or persistence tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::User),
            2 => Some(Self::Agent),
            _ => None,
        }
    }
}

/// One checked message in a product-run conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductConversationMessage {
    role: ProductConversationRole,
    content: String,
}

impl ProductConversationMessage {
    /// Creates one non-empty bounded message.
    ///
    /// # Errors
    ///
    /// Rejects empty, whitespace-only, or oversized content.
    pub fn new(
        role: ProductConversationRole,
        content: String,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&content, MAX_PRODUCT_MESSAGE_BYTES)?;
        Ok(Self { role, content })
    }

    /// Message speaker.
    #[must_use]
    pub const fn role(&self) -> ProductConversationRole {
        self.role
    }

    /// Message text.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// User follow-up for an existing product run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunContinuation {
    run_id: RunId,
    message: String,
}

impl ProductRunContinuation {
    /// Creates one checked follow-up.
    ///
    /// # Errors
    ///
    /// Rejects empty, whitespace-only, or oversized text.
    pub fn new(run_id: RunId, message: String) -> Result<Self, ProductRunMessageError> {
        bounded_text(&message, MAX_PRODUCT_MESSAGE_BYTES)?;
        Ok(Self { run_id, message })
    }

    /// Target run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }

    /// Follow-up text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Query for one exact product-run conversation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunConversationQuery {
    run_id: RunId,
}

impl ProductRunConversationQuery {
    /// Creates an exact conversation query.
    #[must_use]
    pub const fn new(run_id: RunId) -> Self {
        Self { run_id }
    }

    /// Target run.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
}

/// Complete bounded conversation for one product run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunConversation {
    run_id: RunId,
    messages: Vec<ProductConversationMessage>,
}

impl ProductRunConversation {
    /// Creates a bounded conversation in chronological order.
    ///
    /// # Errors
    ///
    /// Rejects a transcript exceeding the retained message limit.
    pub fn new(
        run_id: RunId,
        messages: Vec<ProductConversationMessage>,
    ) -> Result<Self, ProductRunMessageError> {
        if messages.len() > MAX_PRODUCT_MESSAGES {
            return Err(ProductRunMessageError::TooManyMessages);
        }
        Ok(Self { run_id, messages })
    }

    /// Run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }

    /// Chronological transcript.
    #[must_use]
    pub fn messages(&self) -> &[ProductConversationMessage] {
        &self.messages
    }
}
