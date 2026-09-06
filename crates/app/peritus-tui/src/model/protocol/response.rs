//! Request-correlated response projection and retained draft recovery.
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
        if matches!(pending, Some(PendingRequest::ModelQuery)) {
            self.chat.model_picker = false;
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
        let pending = self.pending.remove(&response.request_id());
        let exact_run = match &pending {
            Some(PendingRequest::ProductExactQuery(run_id)) => Some(*run_id),
            _ => None,
        };
        match response.payload() {
            AppResponsePayload::Interaction(snapshot) => {
                if self.chat.run_id != Some(snapshot.snapshot().run_id()) {
                    return Vec::new();
                }
                if matches!(pending, Some(PendingRequest::ChatOpen { .. })) {
                    self.chat.mode = snapshot.mode();
                    self.chat.models = snapshot.models().clone();
                    self.view = View::Conversation;
                }
                self.accept_chat(snapshot.clone());
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
                match TerminalSession::new(*binding, self.limits.max_terminal_chunk_bytes()) {
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
            AppResponsePayload::PromptAccepted(prompt_id) => {
                self.set_prompt_phase(*prompt_id, PromptPhase::Accepted);
                self.notice(NoticeLevel::Info, "prompt response accepted as protocol input");
            }
            AppResponsePayload::Acknowledged(_) => match pending {
                Some(PendingRequest::TerminalDetach) => {
                    self.terminal = None;
                    self.notice(NoticeLevel::Info, "terminal detached");
                }
                Some(PendingRequest::TerminalCancel) => {
                    self.notice(NoticeLevel::Warning, "terminal cancellation was acknowledged");
                }
                _ => {}
            },
            AppResponsePayload::Error(error) => {
                return self.response_error(error, pending.as_ref());
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
}
