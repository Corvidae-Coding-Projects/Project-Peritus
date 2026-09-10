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
        self.workbench_primary_inspection_error(pending, code);
        self.workbench_secondary_inspection_error(pending, code);
    }

    fn workbench_primary_inspection_error(&mut self, pending: &PendingRequest, code: AppErrorCode) {
        match pending {
            PendingRequest::WorkbenchQuery(_) => {
                self.chat.workbench.snapshot = None;
                self.chat.workbench.message =
                    format!("Inspection failed: {}. Draft retained.", code.as_str());
            }
            PendingRequest::WorkbenchPermissions(_) => {
                self.chat.workbench.permissions = None;
                self.chat.workbench.message = format!(
                    "Permission inspection failed: {}. Refresh for current enforced policy; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchInit(_) => {
                self.chat.workbench.init = None;
                self.chat.workbench.message = format!(
                    "Initialization discovery failed: {}. The workspace is unchanged; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchMemory(_) => {
                self.chat.workbench.memory = None;
                self.chat.workbench.message = format!(
                    "Project-guidance inspection failed: {}. Refresh for current state; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchImages(_) => {
                self.chat.workbench.images.page = None;
                self.chat.workbench.message = format!(
                    "Image inspection failed: {}. Refresh for current state; draft retained.",
                    code.as_str()
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
                    "File inspection failed: {}. Refresh for current state; draft retained.",
                    code.as_str()
                );
            }
            _ => {}
        }
    }

    fn workbench_secondary_inspection_error(
        &mut self,
        pending: &PendingRequest,
        code: AppErrorCode,
    ) {
        match pending {
            PendingRequest::WorkbenchQueue(_) => {
                self.chat.workbench.queue = None;
                self.chat.workbench.message = format!(
                    "Queue inspection failed: {}. Refresh to read current state; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchBrief(_) => {
                self.chat.workbench.brief = None;
                self.chat.workbench.message = format!(
                    "Brief inspection failed: {}. Refresh for current state; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchGoal(_) => {
                self.chat.workbench.goal = None;
                self.chat.workbench.message = if code == AppErrorCode::InvalidIdentifier {
                    "No durable goal exists for this conversation. Draft one with /goal <objective>."
                        .to_owned()
                } else {
                    format!(
                        "Goal inspection failed: {}. Refresh for current state; draft retained.",
                        code.as_str()
                    )
                };
            }
            PendingRequest::WorkbenchContext(_) => {
                self.chat.workbench.context_page = None;
                self.chat.workbench.message = format!(
                    "Context inspection failed: {}. Refresh for current state; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchCompaction(_) => {
                self.chat.workbench.compaction_preview = None;
                self.chat.workbench.message = format!(
                    "Compaction preview failed: {}. Prior prompt view remains active; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchReview(_) => {
                if let Some(product) = &mut self.product {
                    product.review.page = None;
                    product.review.message = format!(
                        "Structured review failed: {}. Raw diff remains available.",
                        code.as_str()
                    );
                }
            }
            PendingRequest::WorkbenchResult(_) => {
                if let Some(product) = &mut self.product {
                    product.preview = None;
                }
                self.chat.workbench.message = format!(
                    "Preview result inspection failed: {}. Refresh for current state; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchRewind(_) => {
                self.chat.workbench.rewind_preview = None;
                self.chat.workbench.message = format!(
                    "Rewind preview failed: {}. No workspace bytes changed; draft retained.",
                    code.as_str()
                );
            }
            PendingRequest::WorkbenchCheckpointInspect(_) => {
                self.chat.workbench.checkpoint_receipt = None;
                self.chat.workbench.message = format!(
                    "Checkpoint inspection failed: {}. Refresh for exact historical references; draft retained.",
                    code.as_str()
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
                WorkbenchIntent::AddReview { feedback, .. } => Some((*feedback, draft.clone())),
                _ => None,
            });
        self.chat.workbench.unresolved = None;
        self.chat.workbench.snapshot = None;
        self.chat.workbench.queue = None;
        self.chat.workbench.brief = None;
        self.chat.workbench.goal = None;
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
        if let Some((feedback, draft)) = review_draft {
            self.editor = Some(crate::model::Editor {
                kind: crate::model::EditorKind::ReviewFeedback(feedback),
                title: "Review comment rejected",
                hint: "Refresh the structured diff, then retry or cancel this retained draft.",
                cursor: draft.len(),
                buffer: draft,
            });
        }
    }
}
