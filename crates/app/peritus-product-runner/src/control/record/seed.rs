//! Immutable historical context only. No execution, goal qualification, or effect receipts.
use super::{ControlError, ConversationRecord};
use crate::control::{
    ContextSelections, ConversationId, FileAttachments, ImageAttachments, InputLedger, PromptView,
    PublicReplyReference, TaskBrief,
};
use serde::Deserialize;
use serde::Serialize;

/// Exact source-backed context carried by a non-running conversation branch.
/// Invocation IDs in this value identify immutable past messages, not resumable tool calls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSeed {
    source: ConversationId,
    revision: u64,
    inputs: InputLedger,
    brief: TaskBrief,
    context: ContextSelections,
    prompt_view: PromptView,
    images: ImageAttachments,
    files: FileAttachments,
    replies: Vec<PublicReplyReference>,
}

impl ConversationRecord {
    /// Captures bounded immutable conversation context, excluding runtime authority and effects.
    ///
    /// # Errors
    /// Rejects an invalid source record.
    pub fn historical_seed(&self) -> Result<ConversationSeed, ControlError> {
        self.validate()?;
        Ok(ConversationSeed {
            source: self.id,
            revision: self.revision,
            inputs: self.inputs.historical_snapshot(),
            brief: self.brief.clone(),
            context: self.context.clone(),
            prompt_view: self.prompt_view.clone(),
            images: self.images.clone(),
            files: self.files.historical_snapshot(),
            replies: self.replies.clone(),
        })
    }

    pub(super) fn install_seed(&mut self, seed: &ConversationSeed) {
        self.inputs = seed.inputs.clone();
        self.brief = seed.brief.clone();
        self.context = seed.context.clone();
        self.prompt_view = seed.prompt_view.clone();
        self.images = seed.images.clone();
        self.files = seed.files.clone();
        self.replies.clone_from(&seed.replies);
    }
}

impl ConversationSeed {
    pub(in crate::control) fn matches(&self, source: ConversationId, revision: u64) -> bool {
        self.source == source && self.revision == revision
    }
}
