//! Anchored feedback lifecycle and exact governing input revisions.

use super::{
    BTreeSet, ControlError, ControlText, InputId, InputLedger, InputSelection, InputState,
    InvocationId, MAX_REVIEW_COMMENTS, OperationId, PathBuf, QueueIntent, ReviewAnchor,
    ReviewComment, ReviewCommentState, ReviewFeedback, ReviewLedger,
};
use std::fmt::Write as _;

impl ReviewLedger {
    /// Returns whether no anchored feedback has been accepted.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }
    /// Borrows comments in durable creation order.
    #[must_use]
    pub fn comments(&self) -> &[ReviewComment] {
        &self.comments
    }
    /// Resolves one stable comment identity.
    #[must_use]
    pub fn comment(&self, id: OperationId) -> Option<&ReviewComment> {
        self.comments.iter().find(|comment| comment.id == id)
    }
    /// Returns complete relative paths protected by active leave-alone constraints.
    #[must_use]
    pub fn protected_paths(&self) -> Vec<PathBuf> {
        self.comments
            .iter()
            .filter(|comment| comment.active_constraint())
            .map(|comment| comment.anchor.path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Whether the newest queued anchored feedback explicitly asks for revision work.
    #[must_use]
    pub fn pending_pipeline_permission(&self, inputs: &InputLedger) -> Option<bool> {
        self.comments.iter().rev().find_map(|comment| {
            let item = inputs.latest(comment.input.id())?;
            (item.selection() == comment.input && item.state() == InputState::Queued)
                .then_some(comment.feedback == ReviewFeedback::RequestRevision)
        })
    }

    pub(in crate::control) fn add(
        &self,
        inputs: &InputLedger,
        author: peritus_types::ActorId,
        operation: OperationId,
        anchor: ReviewAnchor,
        feedback: ReviewFeedback,
        message: ControlText<8192>,
    ) -> Result<(Self, InputLedger), ControlError> {
        self.validate(inputs)?;
        anchor.validate()?;
        if self.comments.len() >= MAX_REVIEW_COMMENTS {
            return Err(ControlError::Capacity);
        }
        let id = InputId::new(*operation.as_bytes())?;
        let input = InputSelection::new(id, 1)?;
        let text = prompt(&anchor, feedback, message.as_str(), false)?;
        let next_inputs =
            inputs.apply(author, &QueueIntent::Enqueue { id, text, dependencies: Vec::new() })?;
        let mut next = self.clone();
        next.comments.push(ReviewComment {
            id: operation,
            revision: 1,
            anchor,
            feedback,
            message,
            input,
            state: ReviewCommentState::Open,
            addressed_by: None,
        });
        next.validate(&next_inputs)?;
        Ok((next, next_inputs))
    }

    pub(in crate::control) fn rebind(
        &self,
        inputs: &InputLedger,
        author: peritus_types::ActorId,
        operation: OperationId,
        comment_id: OperationId,
        anchor: ReviewAnchor,
    ) -> Result<(Self, InputLedger), ControlError> {
        self.validate(inputs)?;
        anchor.validate()?;
        let mut next = self.clone();
        let comment = next
            .comments
            .iter_mut()
            .find(|comment| comment.id == comment_id)
            .ok_or(ControlError::NotFound)?;
        if comment.state == ReviewCommentState::Dismissed {
            return Err(ControlError::InvalidInput);
        }
        let text = prompt(&anchor, comment.feedback, comment.message.as_str(), true)?;
        let (next_inputs, input) = revise_input(inputs, author, operation, comment.input, text)?;
        comment.anchor = anchor;
        comment.input = input;
        comment.revision = comment.revision.checked_add(1).ok_or(ControlError::Capacity)?;
        comment.state = ReviewCommentState::Open;
        comment.addressed_by = None;
        next.validate(&next_inputs)?;
        Ok((next, next_inputs))
    }

    pub(in crate::control) fn dismiss(
        &self,
        inputs: &InputLedger,
        author: peritus_types::ActorId,
        operation: OperationId,
        comment_id: OperationId,
    ) -> Result<(Self, InputLedger), ControlError> {
        self.validate(inputs)?;
        let mut next = self.clone();
        let comment = next
            .comments
            .iter_mut()
            .find(|comment| comment.id == comment_id)
            .ok_or(ControlError::NotFound)?;
        if comment.state == ReviewCommentState::Dismissed {
            return Err(ControlError::InvalidInput);
        }
        let latest = inputs.latest(comment.input.id()).ok_or(ControlError::InvalidInput)?;
        if latest.selection() != comment.input {
            return Err(ControlError::StaleRevision);
        }
        let (next_inputs, input) = match latest.state() {
            InputState::Queued | InputState::Held => {
                (inputs.apply(author, &QueueIntent::Withdraw(comment.input))?, comment.input)
            }
            InputState::Incorporated => {
                let id = InputId::new(*operation.as_bytes())?;
                let text = dismissal_prompt(comment)?;
                let selection = InputSelection::new(id, 1)?;
                (
                    inputs.apply(
                        author,
                        &QueueIntent::Correct { original: comment.input, id, text },
                    )?,
                    selection,
                )
            }
            InputState::Superseded | InputState::Withdrawn => {
                return Err(ControlError::InvalidInput);
            }
        };
        comment.input = input;
        comment.revision = comment.revision.checked_add(1).ok_or(ControlError::Capacity)?;
        comment.state = ReviewCommentState::Dismissed;
        comment.addressed_by = None;
        next.validate(&next_inputs)?;
        Ok((next, next_inputs))
    }

    pub(in crate::control) fn observe_reply(
        &mut self,
        inputs: &InputLedger,
        invocation: InvocationId,
        reply: OperationId,
    ) {
        for comment in &mut self.comments {
            if comment.feedback != ReviewFeedback::Explain
                || comment.state != ReviewCommentState::Open
            {
                continue;
            }
            let incorporated = inputs.invocations().iter().any(|binding| {
                binding.invocation() == invocation && binding.items().contains(&comment.input)
            });
            if incorporated {
                comment.state = ReviewCommentState::Addressed;
                comment.addressed_by = Some(reply);
                comment.revision = comment.revision.saturating_add(1);
            }
        }
    }

    pub(in crate::control) fn validate(&self, inputs: &InputLedger) -> Result<(), ControlError> {
        if self.comments.len() > MAX_REVIEW_COMMENTS {
            return Err(ControlError::Capacity);
        }
        let mut ids = BTreeSet::new();
        for comment in &self.comments {
            comment.anchor.validate()?;
            if comment.revision == 0
                || !ids.insert(comment.id)
                || comment.input.id().as_bytes() == &[0; 16]
                || inputs.revisions().iter().all(|input| input.selection() != comment.input)
                || (comment.state == ReviewCommentState::Addressed)
                    != comment.addressed_by.is_some()
            {
                return Err(ControlError::InvalidInput);
            }
        }
        Ok(())
    }
}

fn revise_input(
    inputs: &InputLedger,
    author: peritus_types::ActorId,
    operation: OperationId,
    selected: InputSelection,
    text: ControlText<8192>,
) -> Result<(InputLedger, InputSelection), ControlError> {
    let latest = inputs.latest(selected.id()).ok_or(ControlError::InvalidInput)?;
    if latest.selection() != selected {
        return Err(ControlError::StaleRevision);
    }
    match latest.state() {
        InputState::Queued | InputState::Held => {
            let revision = selected.revision().checked_add(1).ok_or(ControlError::Capacity)?;
            Ok((
                inputs.apply(author, &QueueIntent::Edit { selected, text })?,
                InputSelection::new(selected.id(), revision)?,
            ))
        }
        InputState::Incorporated => {
            let id = InputId::new(*operation.as_bytes())?;
            Ok((
                inputs.apply(author, &QueueIntent::Correct { original: selected, id, text })?,
                InputSelection::new(id, 1)?,
            ))
        }
        InputState::Superseded | InputState::Withdrawn => Err(ControlError::InvalidInput),
    }
}

fn prompt(
    anchor: &ReviewAnchor,
    feedback: ReviewFeedback,
    message: &str,
    rebound: bool,
) -> Result<ControlText<8192>, ControlError> {
    let action = match feedback {
        ReviewFeedback::Explain => {
            "Explain this exact target using read-only current source and diff evidence. Do not modify files or run effectful commands."
        }
        ReviewFeedback::RequestRevision => {
            "Revise this exact target through ordinary work admission, then reacquire checks and independent review for the whole candidate. This feedback grants no additional permission."
        }
        ReviewFeedback::KeepBehavior => {
            "Preserve the described behavior while handling future work. This is a semantic preference, not a hard protected-path rule or permission grant."
        }
        ReviewFeedback::LeaveAlone => {
            "Hard constraint: do not modify the anchored target. The host protects the complete path because this backend cannot safely confine writes to a line range. This narrows authority and grants no permission elsewhere."
        }
    };
    let mut text = String::new();
    let range = anchor.range;
    write!(
        text,
        "Workbench anchored change feedback{}\nAction: {:?}\nInstruction: {}\nUser comment: {}\nRun: {}\nWorkspace: {}\nCandidate SHA-256: {}\nDiff SHA-256: {}\nPath: {}\nTarget: {:?}\nOld range: {},{}\nNew range: {},{}\nBefore blob SHA-256: {}\nAfter blob SHA-256: {}\nContext SHA-256: {}",
        if rebound { " (explicitly rebound by the user)" } else { "" },
        feedback,
        action,
        message,
        hex(anchor.run.as_slice()),
        hex(anchor.workspace.as_slice()),
        hex(anchor.candidate_digest.as_slice()),
        hex(anchor.diff_digest.as_slice()),
        anchor.path.display(),
        anchor.target,
        range.old_start,
        range.old_lines,
        range.new_start,
        range.new_lines,
        hex(anchor.before_blob_digest.as_slice()),
        hex(anchor.after_blob_digest.as_slice()),
        hex(anchor.context_digest.as_slice()),
    )
    .map_err(|_| ControlError::InvalidInput)?;
    ControlText::new(text)
}

fn dismissal_prompt(comment: &ReviewComment) -> Result<ControlText<8192>, ControlError> {
    ControlText::new(format!(
        "The user explicitly dismissed workbench review comment {} at comment revision {}. Remove only that comment's prior instruction or hard constraint. Dismissal grants no new permission and does not accept or qualify the candidate.",
        comment.id, comment.revision,
    ))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut value, byte| {
        let _ = write!(value, "{byte:02x}");
        value
    })
}
