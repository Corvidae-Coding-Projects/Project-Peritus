//! Exact receipt correlation and explicit retry after an uncertain transport outcome.

use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchCommand,
    WorkbenchMode, WorkbenchQuery, WorkbenchSnapshot,
};
use peritus_app_protocol::{WorkbenchIntent, WorkbenchReceipt, WorkbenchReviewFeedback};

mod correlation;
mod error;
mod recovery;
mod rejection;

impl AppModel {
    pub(in crate::model) fn recover_workbench_receipt(&mut self) -> Vec<Effect> {
        if self.resume_latest_pending() {
            return self.resume_latest_conversation();
        }
        if !self.workbench_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some((command, _)) = self.chat.workbench.unresolved.clone() else {
            return if self.chat.workbench.submission.is_some()
                || (self.chat.workbench.selected.is_some()
                    && self.chat.workbench.snapshot.is_none())
            {
                self.discover_workbench_execution()
            } else {
                Vec::new()
            };
        };
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
        let effect = self.request(
            AppRequestPayload::WorkbenchCommand(command.clone()),
            PendingRequest::WorkbenchControl(command),
        );
        if effect.is_some()
            && let Some((_, draft)) = self.chat.workbench.unresolved.as_mut()
            && matches!(self.chat.buffer.trim(), "/sessions retry" | "/queue retry")
        {
            draft.clone_from(&self.chat.buffer);
        }
        effect.into_iter().collect()
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
        if !self.workbench_receipt_matches(command, receipt) {
            self.notice(
                NoticeLevel::Error,
                "Mismatched control receipt; no control state applied.",
            );
            return Vec::new();
        }
        if matches!(command.intent(), WorkbenchIntent::ContinueExecution(_)) {
            return self.observe_continuation_receipt(command, receipt);
        }
        self.complete_receipted_draft(command);
        self.chat.workbench.rejected_control = None;
        if let WorkbenchIntent::ForkConversation(fork) = command.intent()
            && fork.child().workspace() != command.query().workspace()
        {
            // Keep the parent usable until the launcher has resolved the child's actual root
            // and trust. A child selection under the parent's context rejects later controls.
            self.chat.workbench.snapshot = None;
            self.chat.workbench.library = None;
            self.chat.workbench.receipted_revision = receipt.accepted_revision();
            self.chat.workbench.message = format!(
                "Fork saved; no inference started. Open it with /sessions open {} {}.",
                crate::model::format_id(fork.child().conversation().as_bytes()),
                crate::model::format_id(fork.child().workspace().as_bytes()),
            );
            if !self.chat.buffer.is_empty() {
                self.chat.workbench.message.push_str(" Staying here to preserve your new draft.");
                return self.refresh_workbench();
            }
            return vec![Effect::OpenConversation(fork.child())];
        }
        self.select_receipt_conversation(command, receipt);
        if self.chat.workbench.selected == Some(receipt.query()) {
            self.chat.workbench.receipted_revision =
                self.chat.workbench.receipted_revision.max(receipt.accepted_revision());
        }
        if let Some(effects) = self.accept_workbench_chat_receipt(command) {
            return effects;
        }
        if matches!(command.intent(), WorkbenchIntent::CreateConversation(_))
            && self.chat.workbench.snapshot_refresh_command.is_some()
        {
            return self.refresh_selected_snapshot();
        }
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
        if self.chat.workbench.goal_confirm_pending.is_some()
            && matches!(command.intent(), WorkbenchIntent::SetBrief { .. })
        {
            return self.refresh_brief();
        }
        self.refresh_workbench()
    }

    pub(super) fn observe_continuation_receipt(
        &mut self,
        command: &WorkbenchCommand,
        receipt: &WorkbenchReceipt,
    ) -> Vec<Effect> {
        if !matches!(command.intent(), WorkbenchIntent::ContinueExecution(_))
            || !self.workbench_receipt_matches(command, receipt)
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched continuation receipt; launch state remains unresolved.",
            );
            return Vec::new();
        }
        self.chat.workbench.rejected_control = None;
        self.chat.workbench.receipted_revision = self
            .chat
            .workbench
            .receipted_revision
            .max(receipt.accepted_revision());
        self.chat.workbench.message = format!(
            "Continuation accepted at revision {}; checking durable launch ownership.",
            receipt.accepted_revision(),
        );
        self.request(
            AppRequestPayload::QueryWorkbenchContinuationAdmission(command.clone()),
            PendingRequest::WorkbenchContinuationAdmission(command.clone()),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_continuation_admission(
        &mut self,
        command: &WorkbenchCommand,
        admission: peritus_app_protocol::WorkbenchContinuationAdmission,
    ) -> Vec<Effect> {
        let WorkbenchIntent::ContinueExecution(settings) = command.intent() else {
            return Vec::new();
        };
        if admission.operation() != command.operation()
            || admission.query() != command.query()
            || admission.run() != settings.run()
        {
            self.notice(
                NoticeLevel::Error,
                "Mismatched continuation admission; original operation remains unresolved.",
            );
            return Vec::new();
        }
        match admission.state() {
            peritus_app_protocol::WorkbenchContinuationAdmissionState::AcceptedPendingLaunch => {
                self.chat.workbench.message = "Continuation receipt is durable, but launch ownership is still pending. /sessions retry replays the exact original command; draft retained.".to_owned();
                self.notice(
                    NoticeLevel::Info,
                    "Continuation accepted without a live or settled owner; use /sessions retry to recover the exact operation.",
                );
                Vec::new()
            }
            peritus_app_protocol::WorkbenchContinuationAdmissionState::LaunchOwned => {
                self.complete_receipted_draft(command);
                "Session continuation has a live or durably settled owner."
                    .clone_into(&mut self.chat.workbench.message);
                self.settle_workbench_continuation(admission.run())
            }
        }
    }

    fn complete_receipted_draft(&mut self, command: &WorkbenchCommand) {
        if let Some((_, draft)) = self.chat.workbench.unresolved.take()
            && self.chat.buffer == draft
            && !(matches!(command.intent(), WorkbenchIntent::CreateConversation(_))
                && self.chat.workbench.selected == Some(command.query()))
        {
            self.clear_chat_command();
        }
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
        self.notice(
            NoticeLevel::Info,
            "Preview operation accepted. Results refresh automatically while running.",
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
                | WorkbenchIntent::AttachFileSource { .. }
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
        let selected = match command.intent() {
            WorkbenchIntent::ForkConversation(request) => {
                self.chat.workbench.library = None;
                Some(request.child())
            }
            _ => Some(receipt.query()),
        };
        self.select_workbench_conversation(selected);
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
        if self.chat.workbench.snapshot_refresh_command.is_none() {
            self.complete_workbench_inspection();
        }
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
