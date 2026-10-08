//! Composer admission through the same immutable queue and exact receipts as /queue.

use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchIntent,
    WorkbenchQuery,
};
use peritus_app_protocol::{
    ProductInteractionQuery, ProductRunOperationKind, ProductRunOperationState,
    WellKnownProtocolFeature, WorkbenchCommand, WorkbenchExecutionSettings,
    WorkbenchExecutionState, WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputText,
    WorkbenchNewInput, WorkbenchQueueIntent,
};
use peritus_types::RunId;

#[derive(Debug)]
pub(super) struct Submission {
    input: WorkbenchNewInput,
    draft: String,
    settings: WorkbenchExecutionSettings,
    queued: bool,
    stopped: bool,
}

impl AppModel {
    pub(in crate::model) const fn workbench_chat_pending(&self) -> bool {
        self.chat.workbench.submission.is_some() || self.chat.workbench.unresolved.is_some()
    }

    pub(in crate::model) const fn workbench_chat_starting(&self) -> bool {
        self.chat.workbench.submission.is_some()
    }

    pub(in crate::model) fn stop_workbench_submission(&mut self) -> Option<Vec<Effect>> {
        self.chat.workbench.submission.as_mut()?.stopped = true;
        self.notice(
            NoticeLevel::Info,
            "Stop requested. Resolving any pending receipt without starting further work.",
        );
        Some(Vec::new())
    }

    pub(in crate::model::chat) fn workbench_conversation_available(&self) -> bool {
        const REQUIRED: [WellKnownProtocolFeature; 5] = [
            WellKnownProtocolFeature::WorkbenchControl,
            WellKnownProtocolFeature::WorkbenchInputs,
            WellKnownProtocolFeature::WorkbenchExecution,
            WellKnownProtocolFeature::WorkbenchConversation,
            WellKnownProtocolFeature::WorkbenchContinuationReceipts,
        ];
        self.context.is_some()
            && REQUIRED.into_iter().all(|required| {
                self.features.iter().any(|feature| feature.as_str() == required.as_str())
            })
    }

    pub(in crate::model::chat) fn send_workbench_chat(&mut self, text: String) -> Vec<Effect> {
        if !self.workbench_conversation_available() {
            self.notice(NoticeLevel::Warning, "This daemon cannot send chat through the selected durable session. Upgrade/reconnect; draft retained.");
            return Vec::new();
        }
        if self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
            || self.chat.workbench.submission.is_some()
        {
            self.notice(
                NoticeLevel::Info,
                "Waiting for the selected session's input receipt; draft retained.",
            );
            return Vec::new();
        }
        let Ok(text) = WorkbenchInputText::new(text) else {
            self.notice(
                NoticeLevel::Warning,
                "Input must be 1–8192 bytes without terminal controls; draft retained.",
            );
            return Vec::new();
        };
        let Some(providers) = self.chat_providers() else { return Vec::new() };
        let Some(run) = self.ids.run() else { return Vec::new() };
        let Ok(id) = WorkbenchInputId::new(self.ids.bytes(b"workbench-chat-input")) else {
            return Vec::new();
        };
        let creation = self.prepare_chat_conversation(text.as_str());
        if self.chat.workbench.selected.is_none() {
            return Vec::new();
        }
        let input = WorkbenchNewInput::new(
            id,
            text,
            WorkbenchInputOrder::new(Vec::new()).expect("empty order"),
        )
        .expect("validated input");
        self.chat.workbench.submission = Some(Submission {
            draft: self.chat.buffer.clone(),
            input,
            settings: WorkbenchExecutionSettings::new(
                run,
                providers,
                self.chat.mode,
                self.chat.models.clone(),
            ),
            queued: false,
            stopped: false,
        });
        self.chat.selection_anchor = None;
        self.chat.mouse_anchor = None;
        if let Some((query, title)) = creation {
            self.submit_workbench_chat_intent(WorkbenchIntent::CreateConversation(title), query, 0)
        } else {
            self.discover_workbench_execution()
        }
    }

    fn prepare_chat_conversation(
        &mut self,
        text: &str,
    ) -> Option<(WorkbenchQuery, peritus_app_protocol::ConversationTitle)> {
        if self.chat.workbench.selected.is_some() {
            return None;
        }
        let workspace = self.product.as_ref()?.launch.workspace_id();
        let (query, _) = self.new_conversation_binding(workspace)?;
        let source = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = peritus_app_protocol::ConversationTitle::derived_label(
            "",
            &source,
            peritus_app_protocol::CONVERSATION_TITLE_LABEL_BYTES,
        )
        .ok()?;
        self.select_workbench_conversation(Some(query));
        Some((query, title))
    }

    pub(in crate::model) fn discover_workbench_execution(&mut self) -> Vec<Effect> {
        if !self.workbench_conversation_available() {
            return self.refresh_workbench();
        }
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        self.request(
            AppRequestPayload::QueryWorkbenchExecution(query),
            PendingRequest::WorkbenchExecution(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_execution(
        &mut self,
        query: WorkbenchQuery,
        state: &WorkbenchExecutionState,
    ) -> Vec<Effect> {
        if query != state.snapshot().query() || self.chat.workbench.selected != Some(query) {
            return Vec::new();
        }
        self.chat.workbench.snapshot = Some(state.snapshot().clone());
        if self.chat.workbench.submission.is_none() {
            self.complete_workbench_inspection();
        }
        if self.chat.workbench.submission.as_ref().is_some_and(|submission| submission.stopped) {
            self.chat.workbench.submission = None;
            self.notice(
                NoticeLevel::Info,
                "Stopped. Any already accepted input remains saved in the queue.",
            );
            return state.run().map_or_else(Vec::new, |run| self.cancel_workbench_run(run));
        }
        if let Some(submission) = &self.chat.workbench.submission {
            if state.snapshot().archived() {
                self.chat.workbench.submission = None;
                self.notice(
                    NoticeLevel::Warning,
                    "This session is archived; /sessions unarchive before sending. Draft retained.",
                );
                return Vec::new();
            }
            if !submission.queued {
                let intent =
                    WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(submission.input.clone()));
                return self.submit_workbench_chat_intent(
                    intent,
                    query,
                    state.snapshot().revision(),
                );
            }
            if state.run().is_none() {
                let intent = WorkbenchIntent::StartExecution(submission.settings.clone());
                return self.submit_workbench_chat_intent(
                    intent,
                    query,
                    state.snapshot().revision(),
                );
            }
        }
        self.chat.run_id = state.run();
        if let Some(run) = state.run() {
            let pending = self.chat.workbench.submission.as_ref().map_or(
                PendingRequest::ChatOpen { run_id: run },
                |_submission| PendingRequest::WorkbenchChatContinue {
                    run,
                    goal: state.has_goal(),
                },
            );
            self.chat.workbench.open = false;
            return self
                .request(
                    AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run)),
                    pending,
                )
                .into_iter()
                .collect();
        }
        Vec::new()
    }

    fn submit_workbench_chat_intent(
        &mut self,
        intent: WorkbenchIntent,
        query: WorkbenchQuery,
        revision: u64,
    ) -> Vec<Effect> {
        let draft =
            self.chat.workbench.submission.as_ref().map(|submission| submission.draft.clone());
        let effects = self.send_bound_workbench_command(intent, query, revision);
        if let Some(draft) = draft
            && let Some((_, saved)) = self.chat.workbench.unresolved.as_mut()
        {
            *saved = draft;
        }
        effects
    }

    pub(super) fn accept_workbench_chat_receipt(
        &mut self,
        command: &WorkbenchCommand,
    ) -> Option<Vec<Effect>> {
        let submission = self.chat.workbench.submission.as_mut()?;
        match command.intent() {
            WorkbenchIntent::CreateConversation(_) => Some(self.discover_workbench_execution()),
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(input))
                if input == &submission.input =>
            {
                submission.queued = true;
                "Input durably saved to this session; checking execution."
                    .clone_into(&mut self.chat.workbench.message);
                self.chat.workbench.open = false;
                Some(self.discover_workbench_execution())
            }
            WorkbenchIntent::StartExecution(settings) if settings == &submission.settings => {
                let run = settings.run();
                let stopped = submission.stopped;
                self.chat.workbench.submission = None;
                "Session execution started from its durable inputs."
                    .clone_into(&mut self.chat.workbench.message);
                self.chat.run_id = Some(run);
                self.chat.workbench.open = false;
                if stopped {
                    return Some(self.cancel_workbench_run(run));
                }
                Some(
                    self.request(
                        AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run)),
                        PendingRequest::ChatOpen { run_id: run },
                    )
                    .into_iter()
                    .collect(),
                )
            }
            _ => None,
        }
    }

    pub(in crate::model) fn continue_workbench_chat(
        &mut self,
        run: RunId,
        goal: bool,
    ) -> Vec<Effect> {
        if self.chat.run_id != Some(run) {
            return Vec::new();
        }
        if self.chat.workbench.submission.as_ref().is_some_and(|submission| submission.stopped) {
            self.chat.workbench.submission = None;
            return self.cancel_workbench_run(run);
        }
        if goal {
            self.chat.workbench.submission = None;
            self.notice(NoticeLevel::Info, "Input saved to this goal's queue; active work incorporates it at the next request. Use /resume if the goal is paused.");
            return Vec::new();
        }
        let operation = self.chat.snapshot.as_ref().map(|snapshot| snapshot.snapshot().operation());
        if operation.is_some_and(peritus_app_protocol::ProductRunOperation::may_start_execution) {
            let Some(query) = self.chat.workbench.selected else { return Vec::new() };
            let Some(revision) = self
                .chat
                .workbench
                .snapshot
                .as_ref()
                .filter(|snapshot| snapshot.query() == query)
                .map(peritus_app_protocol::WorkbenchSnapshot::revision)
            else {
                return self.discover_workbench_execution();
            };
            let Some(settings) = self
                .chat
                .workbench
                .submission
                .as_ref()
                .map(|submission| submission.settings.clone())
            else {
                return Vec::new();
            };
            return self.submit_workbench_chat_intent(
                WorkbenchIntent::ContinueExecution(settings),
                query,
                revision,
            );
        }
        if operation.is_some_and(|operation| {
            operation.kind() != ProductRunOperationKind::Execution
                || operation.state() == ProductRunOperationState::OutcomeUnknown
        }) {
            self.chat.workbench.submission = None;
            self.notice(
                NoticeLevel::Warning,
                "Input saved, but the current operation must be reconciled before execution can resume.",
            );
            return Vec::new();
        }
        self.chat.workbench.submission = None;
        self.notice(NoticeLevel::Info, "Input saved; the next model request will incorporate it.");
        Vec::new()
    }

    pub(in crate::model) fn reset_workbench_submission(&mut self) {
        self.chat.workbench.submission = None;
    }

    pub(in crate::model) fn settle_workbench_continuation(
        &mut self,
        run: RunId,
    ) -> Vec<Effect> {
        let stopped = self
            .chat
            .workbench
            .submission
            .take()
            .is_some_and(|submission| submission.stopped);
        self.chat.run_id = Some(run);
        self.chat.workbench.open = false;
        if stopped {
            return self.cancel_workbench_run(run);
        }
        self.request(
            AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run)),
            PendingRequest::ChatOpen { run_id: run },
        )
        .into_iter()
        .collect()
    }

    fn cancel_workbench_run(&mut self, run: RunId) -> Vec<Effect> {
        self.chat.run_id = Some(run);
        self.request(
            AppRequestPayload::ControlProductRun(peritus_app_protocol::ProductRunControl::new(
                run,
                peritus_app_protocol::ProductRunControlAction::Cancel,
            )),
            PendingRequest::ProductControl,
        )
        .into_iter()
        .collect()
    }
}
