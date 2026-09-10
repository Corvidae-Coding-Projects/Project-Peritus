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
        self.chat.workbench.snapshot = None;
        self.chat.workbench.checkpoint_receipt = Some(receipt.clone());
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
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
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = None;
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
        let WorkbenchIntent::ApplyRewind(preview) = command.intent() else {
            return Vec::new();
        };
        let revision_matches = match receipt.status() {
            WorkbenchRestoreStatus::RecoveryRequired => {
                receipt.accepted_revision() == command.expected_revision().saturating_add(1)
                    || receipt.accepted_revision() == command.expected_revision().saturating_add(2)
            }
            WorkbenchRestoreStatus::Applied if preview.request().child().is_some() => {
                receipt.accepted_revision() >= command.expected_revision().saturating_add(3)
            }
            WorkbenchRestoreStatus::Applied | WorkbenchRestoreStatus::Conflict => {
                receipt.accepted_revision() == command.expected_revision().saturating_add(2)
            }
        };
        if receipt.restore() != command.operation()
            || receipt.checkpoint() != preview.request().checkpoint()
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
        self.chat.workbench.snapshot = None;
        self.chat.workbench.checkpoint_receipt = None;
        self.chat.workbench.rewind_request = None;
        self.chat.workbench.rewind_preview = None;
        self.chat.workbench.restore_receipt = Some(receipt.clone());
        self.chat.workbench.message = match receipt.status() {
            WorkbenchRestoreStatus::Applied if preview.request().child().is_some() => format!(
                "Rewind settled. Read-only historical conversation branch {} is available in /sessions; {} covered path(s) restored. No execution or authority was resumed.",
                crate::model::format_id(preview.request().child().expect("matched logical branch").as_bytes()),
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
}
