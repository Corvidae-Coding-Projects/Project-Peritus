//! Request-correlated response projection and retained draft recovery.

mod control;

use crate::model::{
    AppModel, AppResponseEnvelope, AppResponsePayload, Effect, NoticeLevel, PendingRequest,
    PromptPhase, TerminalSession, View,
};

impl AppModel {
    pub(in crate::model) fn interrupt_pending_requests(&mut self) {
        self.recover_editor_drafts();
        let prompts = self
            .pending
            .values()
            .filter_map(|pending| match pending {
                PendingRequest::Prompt(prompt) => Some(*prompt),
                _ => None,
            })
            .collect::<Vec<_>>();
        for prompt in prompts {
            self.set_prompt_phase(prompt, PromptPhase::Failed);
        }
    }

    fn response_error(
        &mut self,
        error: &peritus_app_protocol::AppProtocolError,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        if pending.is_none() {
            // A dismissed inspection no longer owns the current screen or its notices.
            return Vec::new();
        }
        if matches!(pending, Some(PendingRequest::ArtifactCancel))
            && error.code() == peritus_app_protocol::AppErrorCode::InvalidIdentifier
        {
            // Completion or an earlier rejection may have already removed this owned transfer.
            return Vec::new();
        }
        if let Some(PendingRequest::TerminalLineInput(binding)) = pending
            && let Some(terminal) = &mut self.terminal
            && terminal.binding() == *binding
        {
            terminal.settle_line_input(false);
        }
        if matches!(pending, Some(PendingRequest::ResumeConversationLibrary(_))) {
            self.fail_latest_resume(&error.actionable_message());
            return Vec::new();
        }
        let absent_goal = matches!(pending, Some(PendingRequest::WorkbenchGoal(_)))
            && error.code() == peritus_app_protocol::AppErrorCode::InvalidIdentifier;
        if let Some(effects) = self.resolve_rejected_control(pending, error.code()) {
            return effects;
        }
        self.workbench_error(pending, error.code());
        if matches!(
            pending,
            Some(
                PendingRequest::WorkbenchExecution(_)
                    | PendingRequest::WorkbenchChatContinue { .. }
                    | PendingRequest::WorkbenchChatStarted { .. }
            )
        ) {
            self.reset_workbench_submission();
        }
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
        if matches!(pending, Some(PendingRequest::ModelQuery(_))) {
            self.chat.close_model_picker();
        }
        if let Some(PendingRequest::ProductMessageBinding { run_id, .. }) = pending {
            if self.chat.run_id == Some(*run_id) {
                self.chat.run_id = None;
                self.chat.binding_checked = None;
                self.select_workbench_conversation(None);
            }
            self.notice(
                NoticeLevel::Error,
                format!("{}. Message draft retained.", error.actionable_message()),
            );
            return Vec::new();
        }
        if let Some(
            PendingRequest::ChatOpen { run_id } | PendingRequest::ChatBinding { run_id, .. },
        ) = pending
        {
            if self.chat.run_id != Some(*run_id) {
                return Vec::new();
            }
            if !matches!(pending, Some(PendingRequest::ChatBinding { opening: false, .. })) {
                self.view = View::Conversation;
            }
            self.notice(
                NoticeLevel::Error,
                format!(
                    "{}. Run and draft retained; retry opening the run or reconnect with Ctrl-R.",
                    error.actionable_message()
                ),
            );
            return Vec::new();
        }
        if let Some(PendingRequest::Prompt(prompt_id)) = pending {
            self.set_prompt_phase(*prompt_id, PromptPhase::Failed);
        }
        self.notice(NoticeLevel::Error, error.actionable_message());
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
        let Some(pending) = self.pending.remove(&response.request_id()) else {
            // Responses only project while their request still owns its continuation. In
            // particular, an expired exact query must never become an unscoped list reply.
            return Vec::new();
        };
        let pending = Some(pending);
        if let Some(editor) = self.pending_editor_drafts.remove(&response.request_id())
            && matches!(response.payload(), AppResponsePayload::Error(_))
        {
            self.restore_editor(editor, false);
        }
        self.pending_started.remove(&response.request_id());
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
            AppResponsePayload::InteractionBinding(binding) => {
                return self.accept_chat_binding(binding, pending);
            }
            AppResponsePayload::Improvements(inbox) => {
                self.notice(NoticeLevel::Info, format!("{} harness improvement suggestions. Inspect them in the GUI inbox or with peritus improvements list.", inbox.candidates().len()));
            }
            AppResponsePayload::WorkbenchCheckpoint(_)
            | AppResponsePayload::WorkbenchCheckpointPage(_)
            | AppResponsePayload::WorkbenchRewindPage(_)
            | AppResponsePayload::WorkbenchRewindPreview(_)
            | AppResponsePayload::WorkbenchRestore(_)
            | AppResponsePayload::WorkbenchRestoreSummary(_)
            | AppResponsePayload::WorkbenchExecution(_)
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
            | AppResponsePayload::WorkbenchPreview(_)
            | AppResponsePayload::WorkbenchContext(_)
            | AppResponsePayload::WorkbenchQueue(_)
            | AppResponsePayload::WorkbenchReceipt(_)
            | AppResponsePayload::Doctor(_) => unreachable!("handled above"),
            AppResponsePayload::Interaction(snapshot) => {
                if matches!(pending, Some(PendingRequest::ProductInteractionQuery)) {
                    self.accept_product_interaction(snapshot.clone());
                    return Vec::new();
                }
                return self.interaction_response(snapshot, pending);
            }
            AppResponsePayload::Models(catalog) => self.accept_model_response(catalog, pending),
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
            AppResponsePayload::TerminalAttached(binding)
            | AppResponsePayload::TerminalPipeAttached(binding) => {
                return self.accept_terminal_response(
                    *binding,
                    matches!(payload, AppResponsePayload::TerminalPipeAttached(_)),
                    pending,
                );
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
            AppResponsePayload::ProductRunObservations(observations) => {
                self.accept_observation_query(observations, exact_run);
            }
            AppResponsePayload::ProductRunSettled(settled) => {
                self.accept_product_settlement(settled);
                self.notice(
                    NoticeLevel::Info,
                    format!("coding run settled: {:?}", settled.settlement().disposition()),
                );
            }
        }
        Vec::new()
    }
    fn interaction_response(
        &mut self,
        snapshot: &peritus_app_protocol::ProductInteractionSnapshot,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        if self.chat.run_id != Some(snapshot.snapshot().run_id()) {
            return Vec::new();
        }
        if matches!(pending, Some(PendingRequest::ChatOpen { .. })) {
            self.chat.mode = snapshot.mode();
            self.view = View::Conversation;
        }
        self.accept_chat(snapshot.clone());
        if let Some(PendingRequest::WorkbenchChatContinue { run, goal, mode }) = pending {
            return self.continue_workbench_chat(*run, *goal, *mode);
        }
        if let Some(PendingRequest::WorkbenchChatStarted { run }) = pending {
            return self.workbench_chat_started(*run);
        }
        if matches!(pending, Some(PendingRequest::ModelUpdate { run_id }) if *run_id == snapshot.snapshot().run_id())
        {
            self.notice(NoticeLevel::Info, "Model and effort selection saved for subsequent model turns; any in-flight turn is unchanged.");
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
            Some(PendingRequest::TerminalDetach(binding)) => {
                if self.terminal.as_ref().is_some_and(|terminal| terminal.binding() == *binding) {
                    self.terminal = None;
                    self.notice(NoticeLevel::Info, "terminal detached");
                }
            }
            Some(PendingRequest::TerminalLineInput(binding)) => {
                if let Some(terminal) = &mut self.terminal
                    && terminal.binding() == *binding
                {
                    terminal.settle_line_input(true);
                }
            }
            Some(PendingRequest::TerminalCancel) => {
                self.notice(NoticeLevel::Warning, "terminal cancellation was acknowledged");
            }
            _ => {}
        }
        Vec::new()
    }
    fn accept_terminal_response(
        &mut self,
        binding: peritus_app_protocol::TerminalBinding,
        pipes: bool,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        if matches!(pending, Some(PendingRequest::TerminalAttach(expected)) if *expected == binding)
        {
            return self.accept_terminal(binding, pipes);
        }
        self.notice(
            NoticeLevel::Error,
            "ignored a terminal attachment that did not match the pending request",
        );
        Vec::new()
    }
    fn accept_terminal(
        &mut self,
        binding: peritus_app_protocol::TerminalBinding,
        pipes: bool,
    ) -> Vec<Effect> {
        match TerminalSession::new(binding, self.limits.max_terminal_chunk_bytes()) {
            Ok(mut terminal) => {
                if pipes {
                    terminal.use_pipes();
                }
                self.terminal = Some(terminal);
                self.view = View::Terminal;
                self.notice(
                    NoticeLevel::Info,
                    "terminal attached; Ctrl-] releases keyboard capture",
                );
                if let Some(viewport) = self.chat.viewport {
                    return self.send_terminal_resize(viewport.width, viewport.height);
                }
            }
            Err(error) => self.notice(NoticeLevel::Error, error.to_string()),
        }
        Vec::new()
    }
}

const fn is_control_payload(payload: &AppResponsePayload) -> bool {
    matches!(
        payload,
        AppResponsePayload::WorkbenchCheckpoint(_)
            | AppResponsePayload::WorkbenchCheckpointPage(_)
            | AppResponsePayload::WorkbenchRewindPage(_)
            | AppResponsePayload::WorkbenchRewindPreview(_)
            | AppResponsePayload::WorkbenchRestore(_)
            | AppResponsePayload::WorkbenchRestoreSummary(_)
            | AppResponsePayload::WorkbenchExecution(_)
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
            | AppResponsePayload::WorkbenchPreview(_)
            | AppResponsePayload::WorkbenchContext(_)
            | AppResponsePayload::WorkbenchQueue(_)
            | AppResponsePayload::WorkbenchReceipt(_)
            | AppResponsePayload::Doctor(_)
    )
}
