//! Thread-safe persisted conversation shared with active model turns.

use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use peritus_app_protocol::{
    MAX_PRODUCT_MESSAGES, ProductConversationMessage, ProductConversationRole,
    ProductRunConversation,
};
use peritus_product_runner::ConversationView;
use peritus_types::RunId;

use super::ProductRunServiceError;

pub(super) struct SharedConversation {
    run_id: RunId,
    messages: RwLock<Vec<ProductConversationMessage>>,
    revision: AtomicU64,
}

impl SharedConversation {
    pub(super) fn new(
        run_id: RunId,
        messages: Vec<ProductConversationMessage>,
    ) -> Result<Arc<Self>, ProductRunServiceError> {
        if messages.is_empty() || messages.len() > MAX_PRODUCT_MESSAGES {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        Ok(Arc::new(Self {
            run_id,
            revision: AtomicU64::new(messages.len() as u64),
            messages: RwLock::new(messages),
        }))
    }

    pub(super) fn append(
        &self,
        role: ProductConversationRole,
        content: impl Into<String>,
    ) -> Result<(), ProductRunServiceError> {
        let message = ProductConversationMessage::new(role, content.into())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let mut messages =
            self.messages.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        if messages.len() >= MAX_PRODUCT_MESSAGES {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        messages.push(message);
        self.revision.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Result<ProductRunConversation, ProductRunServiceError> {
        let messages = self.messages.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        ProductRunConversation::new(self.run_id, messages.clone())
            .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn messages(
        &self,
    ) -> Result<Vec<ProductConversationMessage>, ProductRunServiceError> {
        self.messages
            .read()
            .map(|messages| messages.clone())
            .map_err(|_| ProductRunServiceError::Unavailable)
    }
}

impl ConversationView for SharedConversation {
    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn render(&self) -> String {
        let Ok(messages) = self.messages.read() else {
            return "Conversation temporarily unavailable".to_owned();
        };
        messages
            .iter()
            .map(|message| {
                let speaker = match message.role() {
                    ProductConversationRole::User => "User",
                    ProductConversationRole::Agent => "Peritus",
                };
                format!("{speaker}:\n{}", message.content())
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn followups_advance_revision_and_render_in_chronological_order() {
        let run_id = RunId::new([71; 16]).expect("run id");
        let conversation = SharedConversation::new(
            run_id,
            vec![
                ProductConversationMessage::new(
                    ProductConversationRole::User,
                    "build the game".to_owned(),
                )
                .expect("initial message"),
            ],
        )
        .expect("conversation");
        let original_revision = conversation.revision();
        conversation
            .append(ProductConversationRole::Agent, "Which UI should I use?")
            .expect("agent question");
        conversation.append(ProductConversationRole::User, "Use ratatui").expect("user answer");

        assert_eq!(conversation.revision(), original_revision + 2);
        assert_eq!(conversation.snapshot().expect("snapshot").messages().len(), 3);
        assert_eq!(
            conversation.render(),
            "User:\nbuild the game\n\nPeritus:\nWhich UI should I use?\n\nUser:\nUse ratatui"
        );
    }
}
