//! Reopen exact run destinations without retaining another conversation's input routing.

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use peritus_app_protocol::{
    AppRequestPayload, ProductInteractionBinding, ProductInteractionQuery, WellKnownProtocolFeature,
};
use peritus_types::RunId;

impl AppModel {
    pub(in crate::model) fn open_selected_conversation(&mut self) -> Vec<Effect> {
        if self.chat_mutation_pending() {
            self.notice(NoticeLevel::Info, "Resolving the pending input receipt before changing conversations; draft retained.");
            return Vec::new();
        }
        let Some(run) = self.product.as_ref().and_then(|product| product.selected_run()) else {
            return Vec::new();
        };
        let run_id = run.run_id();
        let workspace = run.workspace_id();
        if self.product.as_ref().is_some_and(|product| product.launch.workspace_id() != workspace) {
            if !self.chat.buffer.is_empty() {
                self.notice(NoticeLevel::Warning, "Your unsent draft belongs to this workspace. Send or clear it before opening a run in another workspace.");
                return Vec::new();
            }
            return vec![Effect::OpenRun { run: run_id, workspace }];
        }
        self.abandon_chat_observations();
        if self.chat.run_id != Some(run_id) {
            self.select_workbench_conversation(None);
            self.chat.binding_checked = None;
        }
        self.chat.workbench.open = false;
        self.chat.run_id = Some(run_id);
        self.chat.snapshot = None;
        self.query_chat_binding(run_id, true).into_iter().collect()
    }

    pub(super) fn needs_chat_binding(&self) -> bool {
        self.chat.run_id.is_some()
            && self.chat.workbench.selected.is_none()
            && self.chat.binding_checked != self.chat.run_id
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchRunBinding.as_str()
            })
    }

    pub(super) fn chat_mutation_pending(&self) -> bool {
        self.workbench_chat_pending()
            || self
                .pending
                .values()
                .any(|pending| matches!(pending, PendingRequest::ModelUpdate { .. }))
    }

    pub(in crate::model) fn abandon_chat_observations(&mut self) {
        self.abandon_activity_pages();
        let requests: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(id, pending)| {
                matches!(
                    pending,
                    PendingRequest::ChatQuery
                        | PendingRequest::ChatOpen { .. }
                        | PendingRequest::ChatBinding { .. }
                        | PendingRequest::ProductMessageBinding { .. }
                )
                .then_some(*id)
            })
            .collect();
        for request in requests {
            self.pending.remove(&request);
            self.pending_started.remove(&request);
        }
    }

    pub(super) fn query_chat_binding(&mut self, run_id: RunId, opening: bool) -> Option<Effect> {
        let query = ProductInteractionQuery::new(run_id);
        if self.chat.binding_checked != Some(run_id)
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchRunBinding.as_str()
            })
        {
            self.request(
                AppRequestPayload::QueryInteractionBinding(query),
                PendingRequest::ChatBinding { run_id, opening },
            )
        } else {
            self.request(
                AppRequestPayload::QueryInteraction(query),
                if opening {
                    PendingRequest::ChatOpen { run_id }
                } else {
                    PendingRequest::ChatQuery
                },
            )
        }
    }

    pub(in crate::model) fn accept_chat_binding(
        &mut self,
        binding: &ProductInteractionBinding,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        let run = binding.interaction().snapshot().run_id();
        if !matches!(
            pending,
            Some(
                PendingRequest::ChatBinding { run_id, .. }
                    | PendingRequest::ProductMessageBinding { run_id, .. },
            )
                if *run_id == run
        ) || self.chat.run_id != Some(run)
        {
            self.notice(
                NoticeLevel::Error,
                "Ignored a conversation binding that does not match the selected run.",
            );
            return Vec::new();
        }
        if self.product.as_ref().is_some_and(|product| {
            product.launch.workspace_id() != binding.interaction().snapshot().workspace_id()
        }) {
            self.notice(
                NoticeLevel::Error,
                "This run belongs to another workspace. Open its project before continuing.",
            );
            return Vec::new();
        }
        let conversation = binding.conversation();
        self.select_workbench_conversation(Some(conversation));
        self.chat.run_id = Some(run);
        self.chat.binding_checked = Some(run);
        if matches!(pending, Some(PendingRequest::ChatBinding { opening: true, .. })) {
            self.chat.workbench.open = false;
            self.view = crate::model::View::Conversation;
        }
        self.accept_chat(binding.interaction().clone());
        if let Some(PendingRequest::ProductMessageBinding { message, .. }) = pending {
            self.view = crate::model::View::Conversation;
            self.chat.buffer.clone_from(message);
            return self.send_chat_message(message.clone());
        }
        Vec::new()
    }
}
