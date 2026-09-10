//! Read-only project initialization discovery and exact reviewed-diff submission.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use peritus_app_protocol::{
    InitDiscoveryRequest, InitProposal, WellKnownProtocolFeature, WorkbenchIntent,
};

impl AppModel {
    pub(super) fn init_command_binding(
        &mut self,
        proposal: &InitProposal,
        workspace: peritus_types::WorkspaceId,
    ) -> Option<(peritus_app_protocol::WorkbenchQuery, u64)> {
        if self.chat.workbench.mode != WorkbenchMode::Init
            || self.chat.workbench.init.as_ref() != Some(proposal)
            || self.chat.workbench.selected != Some(proposal.query())
            || proposal.query().workspace() != workspace
        {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /init before applying its exact proposed diff; draft retained.",
            );
            return None;
        }
        Some((proposal.query(), proposal.revision()))
    }

    pub(in crate::model::chat) fn init_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.init_available() {
            self.notice(
                NoticeLevel::Warning,
                "Project initialization unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first; draft retained.",
            );
            return Vec::new();
        };
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before project initialization; draft retained.",
            );
            return Vec::new();
        }
        self.chat.workbench.mode = WorkbenchMode::Init;
        self.chat.workbench.images.open = false;
        self.chat.workbench.files.open = false;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.open = true;
        match arguments {
            "" => {
                self.chat.workbench.init = None;
                "Inspecting selected local files only; no commands, providers, or writes."
                    .clone_into(&mut self.chat.workbench.message);
                self.refresh_selected_snapshot()
            }
            "apply" => {
                let Some(proposal) = self
                    .chat
                    .workbench
                    .init
                    .as_ref()
                    .filter(|proposal| proposal.query() == query)
                    .cloned()
                else {
                    self.notice(
                        NoticeLevel::Warning,
                        "Inspect /init and review the exact diff before applying it; draft retained.",
                    );
                    return Vec::new();
                };
                self.submit_workbench(WorkbenchIntent::ApplyInitDiff(proposal), query.workspace())
            }
            "decline" => {
                self.chat.workbench.init = None;
                self.chat.workbench.mode = WorkbenchMode::Sessions;
                self.clear_chat_command();
                "Initialization proposal declined locally; the workspace is unchanged."
                    .clone_into(&mut self.chat.workbench.message);
                Vec::new()
            }
            _ => {
                self.notice(NoticeLevel::Warning, "Use /init [apply | decline]; draft retained.");
                Vec::new()
            }
        }
    }

    pub(in crate::model) fn refresh_init(&mut self) -> Vec<Effect> {
        if !self.init_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(snapshot) = self.chat.workbench.snapshot.as_ref() else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /sessions for the current conversation revision before /init.",
            );
            return Vec::new();
        };
        let Ok(request) = InitDiscoveryRequest::new(snapshot.query(), snapshot.revision()) else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::DiscoverInit(request),
            PendingRequest::WorkbenchInit(request),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_init_proposal(
        &mut self,
        request: InitDiscoveryRequest,
        proposal: InitProposal,
    ) {
        if proposal.query() != request.query()
            || proposal.revision() != request.revision()
            || self.chat.workbench.selected != Some(request.query())
            || self.chat.workbench.mode != WorkbenchMode::Init
        {
            return;
        }
        self.chat.workbench.init = Some(proposal);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    fn init_available(&self) -> bool {
        self.workbench_available()
            && self
                .features
                .iter()
                .any(|feature| feature.as_str() == WellKnownProtocolFeature::WorkbenchInit.as_str())
    }
}
