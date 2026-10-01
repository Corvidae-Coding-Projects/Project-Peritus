//! Revision fencing derived from exact history, never from a mutable cached counter.

use super::{ControlError, ControlIntent, ControlOperation, ControlReceipt, ConversationRecord};

/// Replays a conversation while retaining the last revision that changed user-controlled state.
/// Background accounting may advance after an inspected revision without invalidating an edit.
#[derive(Debug, Default)]
pub struct ConversationReplay {
    current: Option<ConversationRecord>,
    edit_revision: u64,
}

impl ConversationReplay {
    /// Borrows the exact latest projection.
    #[must_use]
    pub const fn current(&self) -> Option<&ConversationRecord> {
        self.current.as_ref()
    }

    /// Applies one operation, preserving its original identity and inspected revision.
    ///
    /// # Errors
    /// Rejects competing edits, invalid lifecycle transitions, and mismatched ownership.
    pub fn apply(&mut self, operation: &ControlOperation) -> Result<ControlReceipt, ControlError> {
        let (next, receipt) = ConversationRecord::apply_after_accounting(
            self.current.as_ref(),
            operation,
            self.edit_revision,
        )?;
        if !matches!(
            operation.intent(),
            ControlIntent::ObserveGoalProgress { .. }
                | ControlIntent::ReserveGoalRequest { .. }
                | ControlIntent::CompleteGoalRequest { .. }
                | ControlIntent::ReserveGoalTool { .. }
                | ControlIntent::CompleteGoalTool { .. }
        ) {
            self.edit_revision = next.revision();
        }
        self.current = Some(next);
        Ok(receipt)
    }
}

impl ControlOperation {
    pub(super) const fn can_follow_accounting(&self) -> bool {
        matches!(
            self.intent,
            ControlIntent::Queue(
                crate::control::QueueIntent::Enqueue { .. }
                    | crate::control::QueueIntent::Edit { .. }
                    | crate::control::QueueIntent::Correct { .. }
                    | crate::control::QueueIntent::Hold { .. }
                    | crate::control::QueueIntent::Withdraw(_)
                    | crate::control::QueueIntent::Reorder(_)
            ) | ControlIntent::SetBrief { .. }
                | ControlIntent::UpdateGoalBudget { .. }
                | ControlIntent::ClearGoal { .. }
                | ControlIntent::RenameConversation { .. }
                | ControlIntent::PinConversation { .. }
        )
    }
}
