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

const ROLLOVER_MESSAGES: usize = 64;
const ROLLOVER_NOTICE: &str = "Earlier transcript messages were rolled out of the active context after the conversation reached its durable message bound. Continue from the retained recent exchange.";

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
        let revision = messages
            .iter()
            .filter(|message| message.role() == ProductConversationRole::User)
            .count() as u64;
        Self::new_with_revision(run_id, messages, revision)
    }

    pub(super) fn new_with_revision(
        run_id: RunId,
        messages: Vec<ProductConversationMessage>,
        revision: u64,
    ) -> Result<Arc<Self>, ProductRunServiceError> {
        if messages.is_empty() || messages.len() > MAX_PRODUCT_MESSAGES {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let retained_user_messages = messages
            .iter()
            .filter(|message| message.role() == ProductConversationRole::User)
            .count() as u64;
        if revision < retained_user_messages {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        Ok(Arc::new(Self {
            run_id,
            revision: AtomicU64::new(revision),
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
        compact_for_append(&mut messages)?;
        messages.push(message);
        if role == ProductConversationRole::User {
            self.revision.fetch_add(1, Ordering::Release);
        }
        Ok(())
    }

    pub(super) fn appended(
        &self,
        role: ProductConversationRole,
        content: impl Into<String>,
    ) -> Result<Arc<Self>, ProductRunServiceError> {
        let mut messages = self.messages()?;
        compact_for_append(&mut messages)?;
        messages.push(
            ProductConversationMessage::new(role, content.into())
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
        );
        let revision = self
            .revision
            .load(Ordering::Acquire)
            .saturating_add(u64::from(role == ProductConversationRole::User));
        Self::new_with_revision(self.run_id, messages, revision)
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

fn compact_for_append(
    messages: &mut Vec<ProductConversationMessage>,
) -> Result<(), ProductRunServiceError> {
    if messages.len() < MAX_PRODUCT_MESSAGES {
        return Ok(());
    }
    let remove = ROLLOVER_MESSAGES.min(messages.len().saturating_sub(1));
    messages.drain(1..=remove);
    messages.insert(
        1,
        ProductConversationMessage::new(ProductConversationRole::Agent, ROLLOVER_NOTICE.to_owned())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?,
    );
    Ok(())
}

impl ConversationView for SharedConversation {
    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn render(&self) -> String {
        let Ok(messages) = self.messages.read() else {
            return "Conversation temporarily unavailable".to_owned();
        };
        // Trailing agent entries are daemon-visible observations or an unanswered question. They
        // do not become model input until a later user message makes that exchange part of the
        // governing conversation. This keeps a phase-preserving retry byte-identical to the
        // transcript captured before terminal status publication.
        let through_latest_user = messages
            .iter()
            .rposition(|message| message.role() == ProductConversationRole::User)
            .map_or(0, |index| index + 1);
        messages
            .iter()
            .take(through_latest_user)
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
    fn only_user_followups_advance_revision_and_agent_messages_wait_for_a_reply() {
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
        assert_eq!(conversation.revision(), original_revision);
        assert_eq!(conversation.render(), "User:\nbuild the game");

        conversation.append(ProductConversationRole::User, "Use ratatui").expect("user answer");

        assert_eq!(conversation.revision(), original_revision + 1);
        assert_eq!(conversation.snapshot().expect("snapshot").messages().len(), 3);
        assert_eq!(
            conversation.render(),
            "User:\nbuild the game\n\nPeritus:\nWhich UI should I use?\n\nUser:\nUse ratatui"
        );
    }

    #[test]
    fn full_conversation_rolls_forward_and_preserves_monotonic_input_revision() {
        let run_id = RunId::new([72; 16]).expect("run id");
        let conversation = SharedConversation::new(
            run_id,
            vec![
                ProductConversationMessage::new(
                    ProductConversationRole::User,
                    "initial".to_owned(),
                )
                .expect("initial message"),
            ],
        )
        .expect("conversation");
        for index in 1..MAX_PRODUCT_MESSAGES {
            conversation
                .append(ProductConversationRole::Agent, format!("reply {index}"))
                .expect("bounded reply");
        }

        conversation
            .append(ProductConversationRole::User, "continue after rollover")
            .expect("rollover follow-up");

        let snapshot = conversation.snapshot().expect("conversation snapshot");
        assert!(snapshot.messages().len() < MAX_PRODUCT_MESSAGES);
        assert_eq!(conversation.revision(), 2);
        assert!(snapshot.messages().iter().any(|message| message.content() == ROLLOVER_NOTICE));
        assert_eq!(snapshot.messages().last().unwrap().content(), "continue after rollover");
    }
}
