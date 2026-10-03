//! Presentation-state recovery for rejected or uncertain workbench requests.

use super::{AppModel, PendingRequest, WorkbenchCommand, WorkbenchIntent, preview_intent};
use peritus_app_protocol::AppErrorCode;

impl AppModel {
    pub(in crate::model) fn workbench_error(
        &mut self,
        pending: Option<&PendingRequest>,
        code: AppErrorCode,
    ) {
        let Some(pending) = pending else { return };
        if let PendingRequest::WorkbenchControl(command)
        | PendingRequest::WorkbenchReceipt(command) = pending
        {
            self.workbench_control_error(command, pending, code);
            return;
        }
        self.workbench_primary_inspection_error(pending, code.as_str());
        self.workbench_secondary_inspection_error(
            pending,
            code.as_str(),
            code == AppErrorCode::InvalidIdentifier,
        );
    }

    pub(in crate::model) fn workbench_inspection_timeout(&mut self, pending: &PendingRequest) {
        self.chat.workbench.snapshot_refresh_command = None;
        self.chat.workbench.goal_refresh_command = None;
        self.chat.workbench.goal_confirm_pending = None;
        self.chat.workbench.inspection_draft = None;
        self.workbench_primary_inspection_error(pending, "request timed out");
        self.workbench_secondary_inspection_error(pending, "request timed out", false);
        if matches!(pending, PendingRequest::WorkbenchExecution(_)) {
            "Session inspection timed out. Refresh to retry; draft retained."
                .clone_into(&mut self.chat.workbench.message);
        }
        if matches!(pending, PendingRequest::WorkbenchImagePreview(_)) {
            self.chat.workbench.images.discard_preview();
            "Image preview timed out. Retry the command; draft retained."
                .clone_into(&mut self.chat.workbench.message);
        }
    }

    fn workbench_primary_inspection_error(&mut self, pending: &PendingRequest, detail: &str) {
        match pending {
            PendingRequest::ConversationLibrary(_) => {
                self.chat.workbench.message = format!(
                    "Session lookup failed: {detail}. Current results and draft retained; r retries the search, Esc returns."
                );
            }
            PendingRequest::WorkbenchQuery(_) | PendingRequest::WorkbenchQueueCommand { .. } => {
                self.chat.workbench.snapshot = None;
                self.chat.workbench.snapshot_refresh_command = None;
                self.chat.workbench.files.cancel_pending_preview();
                self.chat.workbench.message =
                    format!("Inspection failed: {detail}. Draft retained.");
            }
            PendingRequest::WorkbenchPermissions(_) => {
                self.chat.workbench.permissions = None;
                self.chat.workbench.message = format!(
                    "Permission inspection failed: {detail}. Refresh for current enforced policy; draft retained."
                );
            }
            PendingRequest::WorkbenchInit(_) => {
                self.chat.workbench.init = None;
                self.chat.workbench.message = format!(
                    "Initialization discovery failed: {detail}. The workspace is unchanged; draft retained."
                );
            }
            PendingRequest::WorkbenchMemory(_) => {
                self.chat.workbench.memory = None;
                self.chat.workbench.message = format!(
                    "Project-guidance inspection failed: {detail}. Refresh for current state; draft retained."
                );
            }
            PendingRequest::WorkbenchImages(_) => {
                self.chat.workbench.images.page = None;
                self.chat.workbench.message = format!(
                    "Image inspection failed: {detail}. Refresh for current state; draft retained."
                );
            }
            PendingRequest::WorkbenchFilePreview(_)
            | PendingRequest::WorkbenchFileImportPreview(_)
            | PendingRequest::WorkbenchFileUpload { .. }
            | PendingRequest::WorkbenchFiles(_) => {
                self.chat.workbench.files.discard_preview();
                self.chat.workbench.files.page = None;
                self.chat.workbench.snapshot = None;
                self.chat.workbench.message = format!(
                    "File inspection failed: {detail}. Refresh for current state; draft retained."
                );
            }
            _ => {}
        }
    }

    fn workbench_secondary_inspection_error(
        &mut self,
        pending: &PendingRequest,
        detail: &str,
        absent_goal: bool,
    ) {
        match pending {
            PendingRequest::WorkbenchQueue(_) => {
                self.chat.workbench.queue = None;
                self.chat.workbench.message = format!(
                    "Queue inspection failed: {detail}. Refresh to read current state; draft retained."
                );
            }
            PendingRequest::WorkbenchBrief(_) => {
                self.chat.workbench.brief = None;
                self.chat.workbench.message = format!(
                    "Brief inspection failed: {detail}. Refresh for current state; draft retained."
                );
            }
            PendingRequest::WorkbenchGoal(_) => {
                self.chat.workbench.goal_refresh_command = None;
                self.chat.workbench.goal = None;
                self.chat.workbench.message = if absent_goal {
                    "No durable goal exists for this conversation. Draft one with /goal <objective>."
                        .to_owned()
                } else {
                    format!(
                        "Goal inspection failed: {detail}. Refresh for current state; draft retained."
                    )
                };
            }
            PendingRequest::WorkbenchContext(_) => {
                self.chat.workbench.context_page = None;
                self.chat.workbench.message = format!(
                    "Context inspection failed: {detail}. Refresh for current state; draft retained."
                );
            }
            PendingRequest::WorkbenchCompaction(_) => {
                self.chat.workbench.compaction_preview = None;
                self.chat.workbench.message = format!(
                    "Compaction preview failed: {detail}. Prior prompt view remains active; draft retained."
                );
            }
            PendingRequest::WorkbenchReview(_) => {
                if let Some(product) = &mut self.product {
                    product.review.page = None;
                    product.review.message =
                        format!("Structured review failed: {detail}. Raw diff remains available.");
                }
            }
            PendingRequest::WorkbenchResult(_) => {
                self.chat.workbench.message = format!(
                    "Preview result inspection failed: {detail}. Refresh for current state; draft retained."
                );
                if let Some(product) = &mut self.product {
                    product.preview_message.clone_from(&self.chat.workbench.message);
                }
            }
            PendingRequest::WorkbenchRewind(_) => {
                self.chat.workbench.rewind_preview = None;
                self.chat.workbench.message = format!(
                    "Rewind preview failed: {detail}. No workspace bytes changed; draft retained."
                );
            }
            PendingRequest::WorkbenchCheckpointInspect(_) => {
                self.chat.workbench.checkpoint_receipt = None;
                self.chat.workbench.message = format!(
                    "Checkpoint inspection failed: {detail}. Refresh for exact historical references; draft retained."
                );
            }
            _ => {}
        }
    }

    fn workbench_control_error(
        &mut self,
        command: &WorkbenchCommand,
        pending: &PendingRequest,
        code: AppErrorCode,
    ) {
        if !self.chat.workbench.unresolved.as_ref().is_some_and(|(expected, _)| expected == command)
        {
            return;
        }
        let terminal_rejection = matches!(pending, PendingRequest::WorkbenchControl(_))
            && matches!(
                code,
                AppErrorCode::StaleRevision
                    | AppErrorCode::IdempotencyConflict
                    | AppErrorCode::MalformedFrame
                    | AppErrorCode::SessionMismatch
                    | AppErrorCode::MissingRequiredFeature
                    | AppErrorCode::ReadOnly
                    | AppErrorCode::InvalidIdentifier
                    | AppErrorCode::LimitExceeded
            );
        if !terminal_rejection {
            self.chat.workbench.message = format!(
                "No receipt confirmed: {}. /sessions retry uses the same operation ID; draft retained.",
                code.as_str()
            );
            return;
        }
        let review_draft =
            self.chat.workbench.unresolved.as_ref().and_then(|(_, draft)| match command.intent() {
                WorkbenchIntent::AddReview { feedback, anchor, .. } => Some((
                    crate::model::product::ReviewDraft {
                        query: command.query(),
                        anchor: anchor.clone(),
                        feedback: *feedback,
                    },
                    draft.clone(),
                )),
                _ => None,
            });
        self.chat.workbench.unresolved = None;
        self.chat.workbench.rejected_control = None;
        self.reset_workbench_submission();
        if matches!(command.intent(), WorkbenchIntent::CreateConversation(_))
            && self.chat.workbench.selected == Some(command.query())
        {
            self.select_workbench_conversation(None);
        }
        self.chat.workbench.snapshot = None;
        self.chat.workbench.queue = None;
        self.chat.workbench.brief = None;
        self.chat.workbench.goal = None;
        self.chat.workbench.goal_refresh_command = None;
        self.chat.workbench.goal_confirm_pending = None;
        self.chat.workbench.permissions = None;
        self.chat.workbench.init = None;
        self.chat.workbench.memory = None;
        self.chat.workbench.images.page = None;
        self.chat.workbench.files.page = None;
        self.chat.workbench.files.discard_preview();
        if preview_intent(command.intent())
            && let Some(product) = &mut self.product
        {
            product.preview = None;
        }
        if matches!(command.intent(), WorkbenchIntent::AttachImage { .. }) {
            self.chat.workbench.images.discard_preview();
        }
        self.chat.workbench.message =
            format!("Rejected: {}. Refresh before a new edit; draft retained.", code.as_str());
        if let Some((target, draft)) = review_draft {
            self.restore_editor(crate::model::Editor {
                kind: crate::model::EditorKind::ReviewFeedback(Box::new(target)),
                title: "Review comment rejected",
                hint: "Ctrl-F refreshes the diff; Ctrl-B explicitly rebinds this draft to the selected target.",
                cursor: draft.len(),
                buffer: draft,
                pasted_command: false,
            }, false);
        }
    }
}
