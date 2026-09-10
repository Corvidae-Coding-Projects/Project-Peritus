//! Exact receipt correlation and explicit retry after an uncertain transport outcome.

use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchCommand,
    WorkbenchMode, WorkbenchQuery, WorkbenchSnapshot,
};
use peritus_app_protocol::{WorkbenchIntent, WorkbenchReceipt, WorkbenchReviewFeedback};

mod error;

impl AppModel {
    pub(in crate::model) fn recover_workbench_receipt(&mut self) -> Vec<Effect> {
        if !self.workbench_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some((command, _)) = self.chat.workbench.unresolved.clone() else { return Vec::new() };
        if !self.control_capability_available(&command) {
            "Original operation unresolved; this connection lacks its required feature. Draft retained.".clone_into(&mut self.chat.workbench.message);
            return Vec::new();
        }
        self.request(
            AppRequestPayload::QueryWorkbenchReceipt(command.clone()),
            PendingRequest::WorkbenchReceipt(command),
        )
        .into_iter()
        .collect()
    }

    pub(super) fn retry_workbench(&mut self) -> Vec<Effect> {
        if self.workbench_request_pending() {
            return Vec::new();
        }
        let Some((command, _)) = self.chat.workbench.unresolved.clone() else {
            self.notice(NoticeLevel::Info, "No uncertain control operation to retry.");
            return Vec::new();
        };
        if !self.control_capability_available(&command) {
            self.notice(NoticeLevel::Warning, "Cannot retry the original operation on this daemon; required feature unavailable. Draft retained.");
            return Vec::new();
        }
        self.request(
            AppRequestPayload::WorkbenchCommand(command.clone()),
            PendingRequest::WorkbenchControl(command),
        )
        .into_iter()
        .collect()
    }

    fn review_receipt_action(&self, intent: &WorkbenchIntent) -> &'static str {
        match intent {
            WorkbenchIntent::AddReview { feedback: WorkbenchReviewFeedback::Explain, .. } => {
                "Read-only explanation started."
            }
            WorkbenchIntent::AddReview {
                feedback: WorkbenchReviewFeedback::RequestRevision,
                ..
            } => "Qualified revision work started.",
            WorkbenchIntent::AddReview { .. } | WorkbenchIntent::DismissReview { .. } => {
                "No inference started."
            }
            WorkbenchIntent::RebindReview { comment, .. } => self
                .product
                .as_ref()
                .and_then(|product| product.review.page.as_ref())
                .and_then(|page| page.comments().iter().find(|entry| entry.id() == *comment))
                .map_or("Associated feedback resumed.", |entry| match entry.feedback() {
                    WorkbenchReviewFeedback::Explain => "Read-only explanation started.",
                    WorkbenchReviewFeedback::RequestRevision => "Qualified revision work started.",
                    WorkbenchReviewFeedback::KeepBehavior | WorkbenchReviewFeedback::LeaveAlone => {
                        "No inference started."
                    }
                }),
            _ => unreachable!("only review intents request a review receipt action"),
        }
    }

    pub(in crate::model) fn accept_workbench_receipt(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) -> Vec<Effect> {
        let preview = preview_intent(command.intent());
        let accepted_revision = if preview {
            Some(command.expected_revision())
        } else {
            command.expected_revision().checked_add(1)
        };
        if receipt.operation() != command.operation()
            || receipt.query() != command.query()
            || accepted_revision != Some(receipt.accepted_revision())
            || !self
                .chat
                .workbench
                .unresolved
                .as_ref()
                .is_some_and(|(expected, _)| expected == command)
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched control receipt; no control state applied.",
            );
            return Vec::new();
        }
        if let Some((_, draft)) = self.chat.workbench.unresolved.take()
            && self.chat.buffer == draft
        {
            self.clear_chat_command();
        }
        self.select_receipt_conversation(command, receipt);
        if preview {
            return self.accept_preview_receipt(command, receipt);
        }
        if self.accept_review_receipt(command, receipt) {
            return self.refresh_review();
        }
        let goal_execution = matches!(
            command.intent(),
            WorkbenchIntent::StartGoal { .. } | WorkbenchIntent::ResumeGoal { .. }
        );
        self.invalidate_receipted_control_state(command.intent());
        self.chat.workbench.message = if goal_execution {
            format!(
                "Durably accepted at revision {}; existing runner execution is admitted subject to goal controls.",
                receipt.accepted_revision()
            )
        } else {
            format!(
                "Durably accepted at revision {}. No inference started.",
                receipt.accepted_revision()
            )
        };
        self.refresh_workbench()
    }

    fn accept_preview_receipt(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) -> Vec<Effect> {
        let run = match command.intent() {
            WorkbenchIntent::StartPreview(profile) => Some(profile.run()),
            _ => self
                .product
                .as_ref()
                .and_then(|product| product.preview.as_ref())
                .map(|page| page.query().run()),
        };
        self.chat.workbench.message = format!(
            "Preview operation durably accepted at control revision {}. Refreshing observed results.",
            receipt.accepted_revision()
        );
        run.map_or_else(Vec::new, |run| {
            self.refresh_preview(peritus_app_protocol::WorkbenchResultQuery::new(
                receipt.query(),
                run,
            ))
        })
    }

    fn accept_review_receipt(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) -> bool {
        if !matches!(
            command.intent(),
            WorkbenchIntent::AddReview { .. }
                | WorkbenchIntent::RebindReview { .. }
                | WorkbenchIntent::DismissReview { .. }
        ) {
            return false;
        }
        self.chat.workbench.snapshot = None;
        let action = self.review_receipt_action(command.intent());
        self.chat.workbench.message = format!(
            "Review change durably accepted at revision {}. {action}",
            receipt.accepted_revision(),
        );
        if let Some(product) = &mut self.product {
            product.review.message.clone_from(&self.chat.workbench.message);
        }
        true
    }

    fn invalidate_receipted_control_state(&mut self, intent: &WorkbenchIntent) {
        if matches!(
            intent,
            WorkbenchIntent::AttachFile { .. }
                | WorkbenchIntent::AttachFileImport { .. }
                | WorkbenchIntent::SelectFile { .. }
        ) {
            self.chat.workbench.files.discard_preview();
            self.chat.workbench.files.list = true;
            self.chat.workbench.files.page = None;
            self.chat.workbench.snapshot = None;
        }
        if matches!(intent, WorkbenchIntent::AttachImage { .. }) {
            self.chat.workbench.images.discard_preview();
            self.chat.workbench.images.list = true;
        }
        if matches!(intent, WorkbenchIntent::ApplyCompaction(_)) {
            self.chat.workbench.mode = WorkbenchMode::Sessions;
            self.chat.workbench.compaction_request = None;
            self.chat.workbench.compaction_preview = None;
            self.chat.workbench.snapshot = None;
        }
        if matches!(intent, WorkbenchIntent::ApplyInitDiff(_)) {
            self.chat.workbench.mode = WorkbenchMode::Sessions;
            self.chat.workbench.init = None;
            self.chat.workbench.snapshot = None;
        }
        if matches!(
            intent,
            WorkbenchIntent::SaveGuidance(_)
                | WorkbenchIntent::ReviseGuidance(_)
                | WorkbenchIntent::PinGuidance(_)
                | WorkbenchIntent::ScopeGuidance(_)
                | WorkbenchIntent::ForgetGuidance(_)
        ) {
            self.chat.workbench.memory = None;
            self.chat.workbench.snapshot = None;
        }
        if matches!(
            intent,
            WorkbenchIntent::StartGoal { .. }
                | WorkbenchIntent::PauseGoal { .. }
                | WorkbenchIntent::ResumeGoal { .. }
                | WorkbenchIntent::UpdateGoalBudget { .. }
                | WorkbenchIntent::ClearGoal { .. }
        ) {
            self.chat.workbench.goal = None;
            self.chat.workbench.goal_clear_pending = false;
        }
        if matches!(intent, WorkbenchIntent::StartGoal { .. }) {
            self.chat.workbench.goal_draft = None;
        }
    }

    fn select_receipt_conversation(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) {
        self.chat.workbench.selected = match command.intent() {
            WorkbenchIntent::ForkConversation(request) => {
                self.chat.workbench.library = None;
                Some(request.child())
            }
            _ => Some(receipt.query()),
        };
    }

    pub(in crate::model) fn accept_workbench_snapshot(
        &mut self,
        query: WorkbenchQuery,
        snapshot: WorkbenchSnapshot,
    ) -> bool {
        if query != snapshot.query() || self.chat.workbench.selected != Some(query) {
            return false;
        }
        self.chat.workbench.snapshot = Some(snapshot);
        true
    }

    fn control_capability_available(&self, command: &WorkbenchCommand) -> bool {
        let required =
            AppRequestPayload::WorkbenchCommand(command.clone()).required_workbench_feature();
        self.context.is_some()
            && required.is_some_and(|required| {
                self.features.iter().any(|feature| feature.as_str() == required.as_str())
            })
    }
}

const fn preview_intent(intent: &WorkbenchIntent) -> bool {
    matches!(
        intent,
        WorkbenchIntent::StartPreview(_)
            | WorkbenchIntent::InteractPreview { .. }
            | WorkbenchIntent::CapturePreview(_)
            | WorkbenchIntent::StopPreview { .. }
            | WorkbenchIntent::CheckPreviewBehavior { .. }
            | WorkbenchIntent::AddArtifactFeedback { .. }
    )
}
