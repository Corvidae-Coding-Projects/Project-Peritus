//! Read-only request candidates. Capturing never incorporates an input or changes its state.

use super::{ControlError, InputLedger, InputSelection, InputState};
use std::collections::{BTreeMap, BTreeSet};

/// Exact input view used while preparing a request; it grants no execution authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputCapture {
    generation: u64,
    pending: Vec<InputSelection>,
    included: Vec<InputSelection>,
    public_replies: Vec<super::InvocationId>,
    conversation: String,
}
impl InputCapture {
    /// Appends exact host-validated file context without changing its input generation.
    /// The caller owns scope checks and immutable source-manifest bindings.
    ///
    /// # Errors
    /// Rejects an oversized combined context; nothing is truncated.
    pub fn with_file_context(mut self, context: &str) -> Result<Self, ControlError> {
        if self.conversation.len().saturating_add(context.len()) > 1024 * 1024 {
            return Err(ControlError::Capacity);
        }
        self.conversation.push_str(context);
        Ok(self)
    }
    pub(in crate::control) fn with_brief(
        mut self,
        brief: &crate::control::TaskBrief,
        inputs: &InputLedger,
    ) -> Result<Self, ControlError> {
        brief.append_to(&mut self.conversation, &self.included, inputs)?;
        Ok(self)
    }
    /// Returns the input generation, including hold, withdrawal and ordering transitions.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Borrows new exact selections, in request order, excluding held/blocked inputs.
    #[must_use]
    pub fn pending(&self) -> &[InputSelection] {
        &self.pending
    }
    /// Borrows every incorporated or pending revision included in the governing view, in order.
    #[must_use]
    pub fn included(&self) -> &[InputSelection] {
        &self.included
    }
    /// Borrows source invocation identities of public replies included in this view.
    #[must_use]
    pub fn public_replies(&self) -> &[super::InvocationId] {
        &self.public_replies
    }
    /// Borrows incorporated history followed by eligible pending instructions.
    #[must_use]
    pub fn conversation(&self) -> &str {
        &self.conversation
    }
}

impl InputLedger {
    /// Captures a deterministic next-request input view without mutating the ledger.
    /// A queued input with a held prerequisite remains visible in the queue but is excluded
    /// from this view. Releasing that prerequisite changes the generation and invalidates it.
    ///
    /// # Errors
    /// Rejects invalid imported state rather than rendering a partially valid conversation.
    pub fn capture(&self) -> Result<InputCapture, ControlError> {
        self.capture_pending(true)
    }

    /// Renders only already incorporated inputs for immutable role-request scaffolding.
    /// Pending text is supplied separately at the guarded D0 boundary, so an acknowledged
    /// withdrawal cannot remain embedded in an earlier prepared role prompt or design.
    ///
    /// # Errors
    /// Rejects invalid durable history.
    pub fn incorporated_conversation(&self) -> Result<String, ControlError> {
        self.capture_pending(false).map(|capture| capture.conversation)
    }

    /// Captures public reply artifacts between their preceding invocation and later user input.
    /// The host verifies each supplied artifact's digest and scope before using this pure view.
    /// Trailing replies are retained but excluded until a later user instruction arrives.
    ///
    /// # Errors
    /// Rejects malformed ledger state or a reply without a matching admitted invocation.
    pub fn capture_with_replies(
        &self,
        replies: &BTreeMap<super::InvocationId, String>,
        include_pending: bool,
    ) -> Result<InputCapture, ControlError> {
        self.capture_with_reply_view(
            replies,
            include_pending,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeMap::new(),
        )
    }

    pub(in crate::control) fn capture_with_reply_view(
        &self,
        replies: &BTreeMap<super::InvocationId, String>,
        include_pending: bool,
        pinned: &BTreeSet<super::InvocationId>,
        excluded: &BTreeSet<super::InvocationId>,
        replacements: &BTreeMap<super::InvocationId, String>,
    ) -> Result<InputCapture, ControlError> {
        let mut captured = self.capture_pending(include_pending)?;
        let total = replies
            .values()
            .try_fold(captured.conversation.len(), |total, text| {
                total.checked_add(text.len()).and_then(|size| size.checked_add(32))
            })
            .ok_or(ControlError::Capacity)?;
        if total > 1024 * 1024 {
            return Err(ControlError::Capacity);
        }
        if replies
            .keys()
            .any(|id| !self.invocations.iter().any(|binding| binding.invocation == *id))
            || pinned.iter().any(|id| !replies.contains_key(id))
            || pinned.iter().any(|id| excluded.contains(id))
        {
            return Err(ControlError::InvalidInput);
        }
        let through = if captured.pending.is_empty() {
            self.invocations.iter().rposition(|binding| !binding.items.is_empty()).unwrap_or(0)
        } else {
            self.invocations.len()
        };
        let mut conversation = String::new();
        for (index, binding) in self.invocations.iter().enumerate() {
            for selected in &binding.items {
                append(
                    &mut conversation,
                    self.latest(selected.id()).ok_or(ControlError::InvalidInput)?.text(),
                );
            }
            if (index < through || pinned.contains(&binding.invocation))
                && !excluded.contains(&binding.invocation)
                && let Some(reply) = replies.get(&binding.invocation)
            {
                conversation.push_str("\n\nPeritus (public reply): ");
                conversation.push_str(
                    replacements.get(&binding.invocation).map_or(reply.as_str(), String::as_str),
                );
                captured.public_replies.push(binding.invocation);
            }
        }
        for selected in &captured.pending {
            append(
                &mut conversation,
                self.latest(selected.id()).ok_or(ControlError::InvalidInput)?.text(),
            );
        }
        captured.conversation = conversation;
        Ok(captured)
    }

    fn capture_pending(&self, include_pending: bool) -> Result<InputCapture, ControlError> {
        self.validate()?;
        let mut available = BTreeSet::new();
        let mut conversation = String::new();
        let mut included = Vec::new();
        for binding in &self.invocations {
            for selected in &binding.items {
                let item = self.latest(selected.id()).ok_or(ControlError::InvalidInput)?;
                append(&mut conversation, item.text());
                included.push(*selected);
                available.insert(selected.id());
            }
        }
        let mut pending = Vec::new();
        for id in self.order.iter().filter(|_| include_pending) {
            let item = self.latest(*id).ok_or(ControlError::InvalidInput)?;
            if item.state() == InputState::Queued
                && item.dependencies().iter().all(|dependency| available.contains(dependency))
            {
                pending.push(item.selection());
                included.push(item.selection());
                append(&mut conversation, item.text());
                available.insert(*id);
            }
        }
        Ok(InputCapture {
            generation: self.generation,
            pending,
            included,
            public_replies: Vec::new(),
            conversation,
        })
    }
}

fn append(conversation: &mut String, text: &str) {
    if !conversation.is_empty() {
        conversation.push_str("\n\n");
    }
    conversation.push_str("User: ");
    conversation.push_str(text);
}
