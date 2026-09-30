//! Identical exact-operation checks for control responses and read-only receipt lookup.

use super::{AppModel, WorkbenchCommand, WorkbenchIntent, WorkbenchReceipt, preview_intent};

impl AppModel {
    pub(super) fn workbench_receipt_matches(
        &self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) -> bool {
        let accepted_revision = if preview_intent(command.intent()) {
            Some(command.expected_revision())
        } else {
            command.expected_revision().checked_add(1)
        };
        let revision_matches = if matches!(
            command.intent(),
            WorkbenchIntent::PauseGoal { .. }
                | WorkbenchIntent::Queue(_)
                | WorkbenchIntent::SetBrief { .. }
                | WorkbenchIntent::AcceptBriefProposal { .. }
                | WorkbenchIntent::UpdateGoalBudget { .. }
                | WorkbenchIntent::ClearGoal { .. }
                | WorkbenchIntent::RenameConversation(_)
                | WorkbenchIntent::PinConversation(_)
        ) {
            receipt.accepted_revision() > command.expected_revision()
        } else {
            accepted_revision == Some(receipt.accepted_revision())
        };
        receipt.operation() == command.operation()
            && receipt.query() == command.query()
            && revision_matches
            && self
                .chat
                .workbench
                .unresolved
                .as_ref()
                .is_some_and(|(expected, _)| expected == command)
    }
}
