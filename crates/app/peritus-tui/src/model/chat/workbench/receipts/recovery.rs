//! Resume admission can precede launch persistence; observation must retain exact retry.

use super::{AppModel, Effect, NoticeLevel, WorkbenchCommand, WorkbenchIntent, WorkbenchReceipt};

impl AppModel {
    pub(in crate::model) fn observe_workbench_receipt(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) -> Vec<Effect> {
        if matches!(command.intent(), WorkbenchIntent::ContinueExecution(_)) {
            return self.observe_continuation_receipt(command, receipt);
        }
        if !matches!(command.intent(), WorkbenchIntent::ResumeGoal { .. }) {
            return self.accept_workbench_receipt(command, receipt);
        }
        if !self.workbench_receipt_matches(command, receipt) {
            self.notice(
                NoticeLevel::Error,
                "Mismatched control receipt; no control state applied.",
            );
            return Vec::new();
        }
        // The journal commits ResumeGoal before the separate launch projection is saved.
        // A read-only lookup proves admission only. Explicit exact replay reconciles that
        // gap, or returns the original receipt without relaunching an existing attempt.
        self.chat.workbench.rejected_control = None;
        self.chat.workbench.receipted_revision =
            self.chat.workbench.receipted_revision.max(receipt.accepted_revision());
        self.chat.workbench.message = format!(
            "Resume accepted at revision {}; execution unconfirmed. /sessions retry reconciles the original operation. No inference started by this lookup; draft retained.",
            receipt.accepted_revision()
        );
        self.notice(
            NoticeLevel::Info,
            "Resume admission recovered; /sessions retry reconciles its exact execution. Draft retained.",
        );
        self.refresh_workbench()
    }
}
