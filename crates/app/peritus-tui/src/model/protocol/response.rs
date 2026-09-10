//! Request-correlated response projection and retained draft recovery.

mod control;

use crate::model::{
    AppModel, AppResponseEnvelope, AppResponsePayload, Effect, NoticeLevel, PendingRequest,
    PromptPhase, TerminalSession, View,
};

impl AppModel {
    fn response_error(
        &mut self,
        error: &peritus_app_protocol::AppProtocolError,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        let absent_goal = matches!(pending, Some(PendingRequest::WorkbenchGoal(_)))
            && error.code() == peritus_app_protocol::AppErrorCode::InvalidIdentifier;
        self.workbench_error(pending, error.code());
        self.image_request_error(pending, error.code());
        if absent_goal {
            return Vec::new();
        }
        if matches!(pending, Some(PendingRequest::Doctor(_)))
            && let Some(panel) = &mut self.chat.doctor
        {
            panel.error = Some(format!(
                "Diagnostic request failed: {}. Draft retained; Esc returns.",
                error.code().as_str()
            ));
        }
        if matches!(pending, Some(PendingRequest::ModelUpdate { .. }))
            && error.code() == peritus_app_protocol::AppErrorCode::MissingRequiredFeature
        {
            self.notice(NoticeLevel::Error,
                "Selected reasoning effort is unsupported by this provider. Prior model/effort retained; choose another level or default.");
            return Vec::new();
        }
        if matches!(pending, Some(PendingRequest::ModelQuery)) {
            self.chat.close_model_picker();
        }
        if let Some(PendingRequest::ChatOpen { run_id }) = pending {
            if self.chat.run_id != Some(*run_id) {
                return Vec::new();
            }
            self.chat.run_id = None;
            self.open_product_message_composer();
            return Vec::new();
        }
        if let Some(PendingRequest::ChatSubmit { run_id, text }) = pending
            && self.chat.run_id == Some(*run_id)
        {
            self.restore_chat_draft(text);
        }
        if let Some(PendingRequest::Prompt(prompt_id)) = pending {
            self.set_prompt_phase(*prompt_id, PromptPhase::Failed);
        }
        self.notice(
            NoticeLevel::Error,
            format!(
                "{} / {} / retry {}{}",
                error.subsystem().as_str(),
                error.code().as_str(),
                error.retry().as_str(),
                error.diagnostic().map(|value| format!(": {}", value.as_str())).unwrap_or_default()
            ),
        );
        Vec::new()
    }
    pub(super) fn handle_response(&mut self, response: &AppResponseEnvelope) -> Vec<Effect> {
        if !self.context_matches(response.context()) {
            return Vec::new();
        }
        if let AppResponsePayload::Acknowledged(ack) = response.payload()
            && ack.request_id() != response.request_id()
        {
            self.notice(
                NoticeLevel::Error,
                "Acknowledgement names another request; original request remains pending.",
            );
            return Vec::new();
        }
        let pending = self.pending.remove(&response.request_id());
        let exact_run = match &pending {
            Some(PendingRequest::ProductExactQuery(run_id)) => Some(*run_id),
            _ => None,
        };
        if is_control_payload(response.payload()) {
            return control::project(self, response.payload(), pending);
        }
        self.general_response(response.payload(), pending.as_ref(), exact_run)
    }
    fn general_response(
        &mut self,
        payload: &AppResponsePayload,
        pending: Option<&PendingRequest>,
        exact_run: Option<peritus_types::RunId>,
    ) -> Vec<Effect> {
        match payload {
            AppResponsePayload::WorkbenchCheckpoint(_)
            | AppResponsePayload::WorkbenchRewindPreview(_)
            | AppResponsePayload::WorkbenchRestore(_)
            | AppResponsePayload::Workbench(_)
            | AppResponsePayload::ConversationLibrary(_)
            | AppResponsePayload::WorkbenchPermissions(_)
            | AppResponsePayload::InitProposal(_)
            | AppResponsePayload::WorkbenchMemory(_)
            | AppResponsePayload::WorkbenchCompactionPreview(_)
            | AppResponsePayload::WorkbenchReview(_)
            | AppResponsePayload::WorkbenchImages(_)
            | AppResponsePayload::WorkbenchFiles(_)
            | AppResponsePayload::WorkbenchFilePreview(_)
            | AppResponsePayload::WorkbenchFileImportPreview(_)
            | AppResponsePayload::WorkbenchImagePreview(_)
            | AppResponsePayload::WorkbenchBrief(_)
            | AppResponsePayload::WorkbenchGoal(_)
            | AppResponsePayload::WorkbenchResult(_)
            | AppResponsePayload::WorkbenchContext(_)
            | AppResponsePayload::WorkbenchQueue(_)
            | AppResponsePayload::WorkbenchReceipt(_)
            | AppResponsePayload::Doctor(_) => unreachable!("handled above"),
            AppResponsePayload::Interaction(snapshot) => {
                if self.chat.run_id != Some(snapshot.snapshot().run_id()) {
                    return Vec::new();
                }
                if matches!(pending, Some(PendingRequest::ChatOpen { .. })) {
                    self.chat.mode = snapshot.mode();
                    self.view = View::Conversation;
                }
                self.accept_chat(snapshot.clone());
                if matches!(pending, Some(PendingRequest::ModelUpdate { run_id }) if *run_id == snapshot.snapshot().run_id())
                {
                    self.notice(NoticeLevel::Info, "Model and effort selection saved for subsequent model turns; any in-flight turn is unchanged.");
                }
            }
            AppResponsePayload::Models(catalog) => self.accept_model_catalog(catalog.clone()),
            AppResponsePayload::SubscriptionStarted(started) => {
                self.subscription = Some(started.subscription_id());
                self.notice(
                    NoticeLevel::Info,
                    format!("live event stream resumed after #{}", started.after().get()),
                );
            }
            AppResponsePayload::DaemonStatus(status) => {
                self.daemon_status = Some(status.clone());
            }
            AppResponsePayload::TerminalAttached(binding) => {
                self.accept_terminal(*binding);
            }
            AppResponsePayload::PromptAccepted(prompt_id) => {
                self.set_prompt_phase(*prompt_id, PromptPhase::Accepted);
                self.notice(NoticeLevel::Info, "prompt response accepted as protocol input");
            }
            AppResponsePayload::Acknowledged(_) => return self.accept_ack(pending),
            AppResponsePayload::Error(error) => {
                return self.response_error(error, pending);
            }
            AppResponsePayload::CommandResult(result) => {
                self.notice(NoticeLevel::Info, format!("command result: {result:?}"));
            }
            AppResponsePayload::ArtifactOpened(metadata) => {
                self.notice(
                    NoticeLevel::Info,
                    format!("artifact transfer opened: {} bytes", metadata.byte_size()),
                );
            }
            AppResponsePayload::ShutdownAccepted(_) => {
                self.notice(NoticeLevel::Warning, "daemon accepted graceful shutdown request");
            }
            AppResponsePayload::ProductRunAccepted(snapshot) => {
                self.accept_product_run(snapshot.clone());
                self.notice(NoticeLevel::Info, format!("coding run: {}", snapshot.status()));
            }
            AppResponsePayload::ProductRuns(snapshots) => {
                self.accept_product_query(snapshots, exact_run);
            }
            AppResponsePayload::ProductRunSettled(settled) => {
                self.accept_product_settlement(settled);
                self.notice(
                    NoticeLevel::Info,
                    format!("coding run settled: {:?}", settled.settlement().disposition()),
                );
            }
            AppResponsePayload::ProductRunSettlements(settled) => {
                self.accept_settlement_query(settled, exact_run);
            }
            AppResponsePayload::ProductRunConversation(conversation) => {
                self.accept_product_conversation(conversation.clone());
            }
        }
        Vec::new()
    }
    fn accept_ack(&mut self, pending: Option<&PendingRequest>) -> Vec<Effect> {
        match pending {
            Some(PendingRequest::WorkbenchFileUpload { transfer, step }) => {
                return self.file_upload_ack(*transfer, *step);
            }
            Some(PendingRequest::WorkbenchImageUpload { transfer, step }) => {
                return self.image_upload_ack(*transfer, *step);
            }
            Some(PendingRequest::TerminalDetach) => {
                self.terminal = None;
                self.notice(NoticeLevel::Info, "terminal detached");
            }
            Some(PendingRequest::TerminalCancel) => {
                self.notice(NoticeLevel::Warning, "terminal cancellation was acknowledged");
            }
            _ => {}
        }
        Vec::new()
    }
    fn accept_terminal(&mut self, binding: peritus_app_protocol::TerminalBinding) {
        match TerminalSession::new(binding, self.limits.max_terminal_chunk_bytes()) {
            Ok(terminal) => {
                self.terminal = Some(terminal);
                self.view = View::Terminal;
                self.notice(
                    NoticeLevel::Info,
                    "terminal attached; Ctrl-] releases keyboard capture",
                );
            }
            Err(error) => self.notice(NoticeLevel::Error, error.to_string()),
        }
    }
}

const fn is_control_payload(payload: &AppResponsePayload) -> bool {
    matches!(
        payload,
        AppResponsePayload::WorkbenchCheckpoint(_)
            | AppResponsePayload::WorkbenchRewindPreview(_)
            | AppResponsePayload::WorkbenchRestore(_)
            | AppResponsePayload::Workbench(_)
            | AppResponsePayload::ConversationLibrary(_)
            | AppResponsePayload::WorkbenchPermissions(_)
            | AppResponsePayload::InitProposal(_)
            | AppResponsePayload::WorkbenchMemory(_)
            | AppResponsePayload::WorkbenchCompactionPreview(_)
            | AppResponsePayload::WorkbenchImages(_)
            | AppResponsePayload::WorkbenchFiles(_)
            | AppResponsePayload::WorkbenchFilePreview(_)
            | AppResponsePayload::WorkbenchFileImportPreview(_)
            | AppResponsePayload::WorkbenchImagePreview(_)
            | AppResponsePayload::WorkbenchBrief(_)
            | AppResponsePayload::WorkbenchGoal(_)
            | AppResponsePayload::WorkbenchReview(_)
            | AppResponsePayload::WorkbenchResult(_)
            | AppResponsePayload::WorkbenchContext(_)
            | AppResponsePayload::WorkbenchQueue(_)
            | AppResponsePayload::WorkbenchReceipt(_)
            | AppResponsePayload::Doctor(_)
    )
}
