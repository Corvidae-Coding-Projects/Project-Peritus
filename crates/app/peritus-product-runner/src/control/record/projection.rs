//! Conversation aggregate inspection, encoding, and invariant validation.

use super::{
    CONTROL_SCHEMA, ControlError, ControlExecution, ConversationId, ConversationRecord, decode,
    encode,
};

impl ConversationRecord {
    /// Returns stable conversation identity.
    #[must_use]
    pub const fn id(&self) -> ConversationId {
        self.id
    }
    /// Returns the current authoritative aggregate revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows the explicitly selected title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.title.as_str()
    }
    /// Returns library pin state.
    #[must_use]
    pub const fn pinned(&self) -> bool {
        self.pinned
    }
    /// Returns reversible library archive state.
    #[must_use]
    pub const fn archived(&self) -> bool {
        self.archived
    }
    /// Borrows immutable public reply references in publication order.
    #[must_use]
    pub fn replies(&self) -> &[crate::control::PublicReplyReference] {
        &self.replies
    }
    /// Borrows retained user checkpoints in publication order.
    #[must_use]
    pub fn checkpoints(&self) -> &[crate::control::UserCheckpoint] {
        &self.checkpoints
    }
    /// Borrows durable restore operation receipts in publication order.
    #[must_use]
    pub fn restores(&self) -> &[crate::control::RestoreOperation] {
        &self.restores
    }
    /// Borrows durable input history and exact request incorporation bindings.
    #[must_use]
    pub const fn inputs(&self) -> &crate::control::InputLedger {
        &self.inputs
    }
    /// Borrows user-confirmed field bindings without promoting model output into requirements.
    #[must_use]
    pub const fn brief(&self) -> &crate::control::TaskBrief {
        &self.brief
    }
    /// Borrows explicit user context preferences.
    #[must_use]
    pub const fn context(&self) -> &crate::control::ContextSelections {
        &self.context
    }
    /// Borrows the current deterministic prompt-view generation.
    #[must_use]
    pub const fn prompt_view(&self) -> &crate::control::PromptView {
        &self.prompt_view
    }
    /// Borrows immutable imports and current future-request selection preferences.
    #[must_use]
    pub const fn images(&self) -> &crate::control::ImageAttachments {
        &self.images
    }
    /// Borrows retained file versions and current selection, without reading any source path.
    #[must_use]
    pub const fn files(&self) -> &crate::control::FileAttachments {
        &self.files
    }
    /// Selects image bytes under both attachment selection and explicit context preferences.
    #[must_use]
    pub fn eligible_images(
        &self,
        included: &[crate::control::InputSelection],
    ) -> Vec<&crate::control::ImageAttachment> {
        self.images
            .entries()
            .iter()
            .filter(|entry| {
                let target = crate::control::ContextTarget::Image(entry.image().operation());
                let preference = self.context.preference(target);
                included.iter().any(|input| input.id() == entry.image().input())
                    && preference != Some(crate::control::ContextPreference::Excluded)
                    && (entry.selected()
                        || preference == Some(crate::control::ContextPreference::Pinned))
            })
            .map(crate::control::ImageSelection::image)
            .collect()
    }
    /// Selects file versions under both attachment selection and explicit context preferences.
    #[must_use]
    pub fn eligible_files(
        &self,
        included: &[crate::control::InputSelection],
    ) -> Vec<&crate::control::FileSelection> {
        self.files
            .entries()
            .iter()
            .filter(|entry| {
                let target = crate::control::ContextTarget::File(entry.file().operation());
                let preference = self.context.preference(target);
                included.iter().any(|input| input.id() == entry.file().input())
                    && preference != Some(crate::control::ContextPreference::Excluded)
                    && (entry.selected()
                        || preference == Some(crate::control::ContextPreference::Pinned))
            })
            .collect()
    }

    /// Borrows digest-bound review history and active hard constraints.
    #[must_use]
    pub const fn reviews(&self) -> &crate::control::ReviewLedger {
        &self.reviews
    }

    /// Captures public inputs, eligible replies and current user-confirmed brief fields.
    ///
    /// # Errors
    /// Rejects invalid source bindings or a complete mandatory view exceeding its byte ceiling.
    pub fn capture_with_replies(
        &self,
        replies: &std::collections::BTreeMap<crate::control::InvocationId, String>,
        include_pending: bool,
    ) -> Result<crate::control::InputCapture, ControlError> {
        let pinned: std::collections::BTreeSet<crate::control::InvocationId> = self
            .context
            .entries()
            .iter()
            .filter_map(|entry| match (entry.target(), entry.preference()) {
                (
                    crate::control::ContextTarget::PublicReply(id),
                    crate::control::ContextPreference::Pinned,
                ) => Some(id),
                _ => None,
            })
            .collect();
        let excluded: std::collections::BTreeSet<crate::control::InvocationId> = self
            .context
            .entries()
            .iter()
            .filter_map(|entry| match (entry.target(), entry.preference()) {
                (
                    crate::control::ContextTarget::PublicReply(id),
                    crate::control::ContextPreference::Excluded,
                ) => Some(id),
                _ => None,
            })
            .collect();
        let replacements: std::collections::BTreeMap<crate::control::InvocationId, String> = self
            .prompt_view
            .entries()
            .iter()
            .filter(|entry| !pinned.contains(&entry.invocation()))
            .map(|entry| (entry.invocation(), entry.replacement().to_owned()))
            .collect();
        self.inputs
            .capture_with_reply_view(replies, include_pending, &pinned, &excluded, &replacements)?
            .with_brief(&self.brief, &self.inputs)
    }
    /// Borrows the governed execution lineage, if explicitly admitted.
    #[must_use]
    pub const fn execution(&self) -> Option<&ControlExecution> {
        self.execution.as_ref()
    }
    /// Borrows the durable bounded goal and its cumulative accounting, if one was confirmed.
    #[must_use]
    pub const fn goal(&self) -> Option<&crate::control::GoalRecord> {
        self.goal.as_ref()
    }
    /// Returns the owner binding without disclosing credential material.
    #[must_use]
    pub const fn owner_bytes(&self) -> &[u8; 16] {
        &self.owner
    }
    /// Returns selected workspace identity bytes.
    #[must_use]
    pub const fn workspace_bytes(&self) -> &[u8; 16] {
        &self.workspace
    }
    /// Decodes a bounded current-state root, rejecting invalid generations and identities.
    ///
    /// # Errors
    /// Rejects oversized, malformed, or unsupported state without changing any prior state.
    pub fn parse(bytes: &[u8]) -> Result<Self, ControlError> {
        let value: Self = decode(bytes)?;
        value.validate()?;
        Ok(value)
    }
    /// Encodes the exact validated current projection for atomic journal publication.
    ///
    /// # Errors
    /// Rejects invalid state or a byte/encoding limit.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ControlError> {
        self.validate()?;
        encode(self)
    }
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        self.inputs.validate()?;
        self.brief.validate(&self.inputs)?;
        self.context.validate(self)?;
        self.prompt_view.validate(&self.replies)?;
        self.images.validate(&self.inputs)?;
        self.files.validate(&self.inputs)?;
        self.reviews.validate(&self.inputs)?;
        self.validate_replies()?;
        if self.schema != CONTROL_SCHEMA || self.minimum_reader != CONTROL_SCHEMA {
            return Err(ControlError::UnsupportedSchema);
        }
        if self.revision == 0 || self.owner == [0; 16] || self.workspace == [0; 16] {
            return Err(ControlError::InvalidInput);
        }
        if self.execution.as_ref().is_some_and(|execution| execution.run == [0; 16]) {
            return Err(ControlError::InvalidInput);
        }
        if let Some(goal) = &self.goal {
            goal.validate()?;
            if self.execution.as_ref().is_none_or(|execution| {
                execution.start_operation != goal.id() || &execution.run != goal.run_bytes()
            }) {
                return Err(ControlError::InvalidInput);
            }
        }
        Ok(())
    }

    fn validate_replies(&self) -> Result<(), ControlError> {
        if self.replies.len() > 1024 {
            return Err(ControlError::Capacity);
        }
        let mut previous = None;
        let mut operations = std::collections::BTreeSet::new();
        for reply in &self.replies {
            crate::control::PublicReplyReference::new(
                reply.operation(),
                reply.after_invocation(),
                reply.digest().into_bytes(),
                reply.bytes(),
            )?;
            let index = self
                .inputs
                .invocations()
                .iter()
                .position(|binding| binding.invocation() == reply.after_invocation())
                .ok_or(ControlError::InvalidInput)?;
            if previous.is_some_and(|prior| prior >= index) || !operations.insert(reply.operation())
            {
                return Err(ControlError::InvalidInput);
            }
            previous = Some(index);
        }
        Ok(())
    }
}
