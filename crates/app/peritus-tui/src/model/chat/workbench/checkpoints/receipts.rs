//! Exact correlated checkpoint and restore responses.
use super::{
    AppModel, Effect, NoticeLevel, WorkbenchCheckpointReceipt, WorkbenchCommand, WorkbenchIntent,
    WorkbenchMode, WorkbenchRestoreReceipt, WorkbenchRestoreStatus, WorkbenchRewindRequest,
};

impl AppModel {
    pub(in crate::model) fn accept_workbench_checkpoint(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchCheckpointReceipt,
    ) -> Vec<Effect> {
        let WorkbenchIntent::CreateCheckpoint(name) = command.intent() else {
            return Vec::new();
        };
        let identity_matches = receipt.checkpoint() == command.operation();
        let scope_matches = receipt.query() == command.query();
        let revision_matches =
            receipt.accepted_revision() == command.expected_revision().saturating_add(1);
        let command_matches = self
            .chat
            .workbench
            .unresolved
            .as_ref()
            .is_some_and(|(expected, _)| expected == command);
        if !(identity_matches
            && scope_matches
            && revision_matches
            && receipt.name() == name
            && command_matches)
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched checkpoint receipt; no checkpoint state accepted by this client.",
            );
            return Vec::new();
        }
        self.clear_checkpoint_command(command);
        self.open_checkpoint_panel();
        self.chat.workbench.selected = Some(receipt.query());
        self.chat.workbench.receipted_revision =
            self.chat.workbench.receipted_revision.max(receipt.accepted_revision());
        self.chat.workbench.snapshot = None;
        self.chat.workbench.checkpoint_receipt = Some(receipt.clone());
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = None;
        self.chat.workbench.message = format!(
            "Checkpoint durably captured: {} covered path(s), {} visible exclusion(s).",
            receipt.paths().len(),
            receipt.exclusions().len()
        );
        self.refresh_workbench()
    }

    pub(in crate::model) fn accept_workbench_checkpoint_inspection(
        &mut self,
        request: &WorkbenchRewindRequest,
        receipt: &WorkbenchCheckpointReceipt,
    ) {
        let current_snapshot_matches =
            self.chat.workbench.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.query() == request.query() && snapshot.revision() == request.revision()
            });
        let checkpoint_exists_at_current_revision =
            receipt.accepted_revision() <= request.revision();
        if receipt.checkpoint() != request.checkpoint()
            || receipt.query() != request.query()
            || !checkpoint_exists_at_current_revision
            || self.chat.workbench.selected != Some(request.query())
            || self.chat.workbench.mode != WorkbenchMode::Checkpoints
            || !current_snapshot_matches
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched checkpoint inspection; no checkpoint state accepted by this client.",
            );
            return;
        }
        self.chat.workbench.checkpoint_receipt = Some(receipt.clone());
        self.complete_workbench_inspection();
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = None;
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message = format!(
            "Checkpoint loaded: {} covered path(s), {} visible exclusion(s).",
            receipt.paths().len(),
            receipt.exclusions().len()
        );
    }

    pub(in crate::model) fn accept_workbench_restore(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchRestoreReceipt,
    ) -> Vec<Effect> {
        let request = match command.intent() {
            WorkbenchIntent::ApplyRewind(preview) => preview.request(),
            WorkbenchIntent::ConfirmRewind(confirmation) => confirmation.request(),
            _ => return Vec::new(),
        };
        let revision_matches = match receipt.status() {
            WorkbenchRestoreStatus::RecoveryRequired => {
                receipt.accepted_revision() == command.expected_revision().saturating_add(1)
                    || receipt.accepted_revision() == command.expected_revision().saturating_add(2)
            }
            WorkbenchRestoreStatus::Applied if request.child().is_some() => {
                receipt.accepted_revision() >= command.expected_revision().saturating_add(3)
            }
            WorkbenchRestoreStatus::Applied | WorkbenchRestoreStatus::Conflict => {
                receipt.accepted_revision() == command.expected_revision().saturating_add(2)
            }
        };
        let receipt_restore_id = receipt.restore();
        let command_operation_id = command.operation();
        if receipt_restore_id != command_operation_id
            || receipt.checkpoint() != request.checkpoint()
            || receipt.query() != command.query()
            || !revision_matches
            || !self
                .chat
                .workbench
                .unresolved
                .as_ref()
                .is_some_and(|(expected, _)| expected == command)
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched restore receipt; refresh before any further control action.",
            );
            return Vec::new();
        }
        self.clear_checkpoint_command(command);
        self.open_checkpoint_panel();
        self.chat.workbench.selected = Some(receipt.query());
        self.chat.workbench.receipted_revision =
            self.chat.workbench.receipted_revision.max(receipt.accepted_revision());
        self.chat.workbench.snapshot = None;
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = Some(receipt.clone());
        self.chat.workbench.restore_summary = None;
        self.chat.workbench.message = match receipt.status() {
            WorkbenchRestoreStatus::Applied if request.child().is_some() => format!(
                "Rewind settled. Read-only historical conversation branch {} is available in /sessions; {} covered path(s) restored. No execution or authority was resumed.",
                crate::model::format_id(request.child().expect("matched logical branch").as_bytes()),
                receipt.restored().len()
            ),
            WorkbenchRestoreStatus::Applied => format!(
                "Restore durably applied to {} covered path(s); unrelated files were outside the transaction.",
                receipt.restored().len()
            ),
            WorkbenchRestoreStatus::Conflict => format!(
                "Restore durably stopped on {} conflict(s); no workspace bytes were changed.",
                receipt.conflicts().len()
            ),
            WorkbenchRestoreStatus::RecoveryRequired => {
                "Restore requires recovery inspection; use the retained recovery checkpoint and transaction receipt before continuing."
                    .to_owned()
            }
        };
        self.refresh_workbench()
    }

    pub(in crate::model) fn accept_workbench_restore_summary(
        &mut self,
        command: &WorkbenchCommand,
        summary: &peritus_app_protocol::WorkbenchRestoreSummary,
    ) -> Vec<Effect> {
        let request = match command.intent() {
            WorkbenchIntent::ConfirmRewind(confirmation)
                if confirmation.preview_digest() == summary.fingerprint() =>
            {
                confirmation.request()
            }
            _ => return Vec::new(),
        };
        let revision_matches = match summary.status() {
            WorkbenchRestoreStatus::RecoveryRequired => {
                summary.accepted_revision() == command.expected_revision().saturating_add(1)
                    || summary.accepted_revision() == command.expected_revision().saturating_add(2)
            }
            WorkbenchRestoreStatus::Applied if request.child().is_some() => {
                summary.accepted_revision() >= command.expected_revision().saturating_add(3)
            }
            WorkbenchRestoreStatus::Applied | WorkbenchRestoreStatus::Conflict => {
                summary.accepted_revision() == command.expected_revision().saturating_add(2)
            }
        };
        let summary_restore_id = summary.restore();
        let command_operation_id = command.operation();
        if summary_restore_id != command_operation_id
            || summary.checkpoint() != request.checkpoint()
            || summary.query() != command.query()
            || !revision_matches
            || !self
                .chat
                .workbench
                .unresolved
                .as_ref()
                .is_some_and(|(expected, _)| expected == command)
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched restore summary; refresh before any further control action.",
            );
            return Vec::new();
        }
        self.clear_checkpoint_command(command);
        self.open_checkpoint_panel();
        self.chat.workbench.selected = Some(summary.query());
        self.chat.workbench.receipted_revision =
            self.chat.workbench.receipted_revision.max(summary.accepted_revision());
        self.chat.workbench.snapshot = None;
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
        self.chat.workbench.restore_summary = Some(summary.clone());
        self.chat.workbench.message = match summary.status() {
            WorkbenchRestoreStatus::Applied => format!(
                "Rewind settled: {} covered path(s) restored. The original conversation history and cumulative accounting remain preserved.",
                summary.restored_paths()
            ),
            WorkbenchRestoreStatus::Conflict => format!(
                "Rewind retained {} conflicting path(s) unchanged. Review the paged checkpoint coverage before another control action.",
                summary.conflicting_paths()
            ),
            WorkbenchRestoreStatus::RecoveryRequired => {
                String::from("Rewind requires recovery inspection before another control action.")
            }
        };
        self.refresh_workbench()
    }
}
