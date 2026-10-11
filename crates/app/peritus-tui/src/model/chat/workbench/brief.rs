//! User-confirmed field edits retain the draft until an exact durable receipt arrives.

mod pages;

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use peritus_app_protocol::{
    ControlOperationId, WellKnownProtocolFeature, WorkbenchBrief, WorkbenchBriefField,
    WorkbenchInputText, WorkbenchIntent, WorkbenchQuery,
};

impl AppModel {
    pub(in crate::model::chat) fn brief_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.brief_available() {
            self.notice(
                NoticeLevel::Warning,
                "Task brief unavailable/offline; /reconnect or upgrade daemon. Draft retained.",
            );
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else {
            return self
                .create_command_conversation("New conversation", format!("/brief {arguments}"));
        };
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Resolve the pending workbench request before editing the brief; draft retained.",
            );
            return Vec::new();
        }
        if arguments.is_empty() {
            self.chat.workbench.mode = WorkbenchMode::Brief;
            self.chat.workbench.images.open = false;
            self.chat.workbench.context_mode = None;
            self.chat.workbench.open = true;
            self.chat.workbench.brief = None;
            "Reading user-confirmed fields; no inference."
                .clone_into(&mut self.chat.workbench.message);
            return self.refresh_brief();
        }
        let (action, remainder) = arguments
            .split_once(char::is_whitespace)
            .map_or((arguments, ""), |(action, text)| (action, text.trim()));
        if self.brief_pages_available() && matches!(action, "next" | "previous" | "show" | "text") {
            return self.navigate_brief_pages(action, remainder);
        }
        if action == "accept" {
            let Some((field, proposal)) = remainder.split_once(char::is_whitespace) else {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /brief accept <field> <32 hexadecimal proposal ID>; draft retained.",
                );
                return Vec::new();
            };
            let Some(field) = brief_field(field) else {
                self.notice(
                    NoticeLevel::Warning,
                    "Brief field must be objective, acceptance, constraints, or assumptions; draft retained.",
                );
                return Vec::new();
            };
            let Some(proposal) = crate::model::decode_hex_16(proposal.trim())
                .and_then(|bytes| ControlOperationId::new(bytes).ok())
            else {
                self.notice(
                    NoticeLevel::Warning,
                    "Proposal ID must be exactly 32 hexadecimal characters; draft retained.",
                );
                return Vec::new();
            };
            if self.brief_pages_available() {
                return self.accept_paged_proposal(field, proposal);
            }
            let Some(source) =
                self.chat.workbench.brief.as_ref().filter(|brief| brief.query() == query).and_then(
                    |brief| brief.proposals().iter().find(|source| source.operation() == proposal),
                )
            else {
                self.notice(
                    NoticeLevel::Warning,
                    "Inspect /brief and select an exact listed proposal before acceptance; draft retained.",
                );
                return Vec::new();
            };
            return self.submit_workbench(
                WorkbenchIntent::AcceptBriefProposal { field, proposal, digest: source.digest() },
                query.workspace(),
            );
        }
        let Some(field) = brief_field(action) else {
            self.notice(NoticeLevel::Warning, "Use /brief [objective | acceptance | constraints | assumptions] <confirmed text>, or /brief accept <field> <proposal ID>; draft retained.");
            return Vec::new();
        };
        let Ok(text) = WorkbenchInputText::new(remainder.to_owned()) else {
            self.notice(
                NoticeLevel::Warning,
                "Brief text must be 1–8192 bytes without terminal controls; draft retained.",
            );
            return Vec::new();
        };
        self.submit_workbench(WorkbenchIntent::SetBrief { field, text }, query.workspace())
    }

    pub(super) fn refresh_brief(&mut self) -> Vec<Effect> {
        if !self.brief_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        self.chat.workbench.brief_page = None;
        self.chat.workbench.brief_body = None;
        self.chat.workbench.brief_body_history.clear();
        if self.brief_pages_available() {
            return self.request_brief_page(
                peritus_app_protocol::WorkbenchBriefPageRequest::new(query, 0, 0, 0)
                    .expect("initial page"),
            );
        }
        self.request(
            AppRequestPayload::QueryWorkbenchBrief(query),
            PendingRequest::WorkbenchBrief(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_brief(
        &mut self,
        query: WorkbenchQuery,
        brief: WorkbenchBrief,
    ) -> Vec<Effect> {
        if brief.query() != query
            || self.chat.workbench.selected != Some(query)
            || (self.chat.workbench.mode != WorkbenchMode::Brief && !self.chat.workbench.goal_mode)
        {
            return Vec::new();
        }
        let confirmed_objective = self.chat.workbench.goal_confirm_pending.take();
        let continue_goal = confirmed_objective.is_some();
        let exact_objective = self.chat.workbench.goal_draft.as_ref().is_some_and(|draft| {
            confirmed_objective.as_ref() == Some(draft.objective())
                && brief.entries().iter().any(|entry| {
                    entry.field() == WorkbenchBriefField::Objective
                        && entry.source().text() == draft.objective()
                        && matches!(
                            entry.source().state(),
                            peritus_app_protocol::WorkbenchInputState::Queued
                                | peritus_app_protocol::WorkbenchInputState::Incorporated
                        )
                })
        });
        self.chat.workbench.brief = Some(brief);
        self.complete_workbench_inspection();
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
        if continue_goal && exact_objective {
            return self.confirm_goal();
        }
        if continue_goal {
            self.notice(NoticeLevel::Warning, "The objective changed during confirmation; review /brief and confirm the goal again. Draft retained.");
        }
        Vec::new()
    }

    fn brief_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchBrief.as_str()
            })
    }
}

fn brief_field(value: &str) -> Option<WorkbenchBriefField> {
    match value {
        "objective" => Some(WorkbenchBriefField::Objective),
        "acceptance" => Some(WorkbenchBriefField::Acceptance),
        "constraints" => Some(WorkbenchBriefField::Constraints),
        "assumptions" => Some(WorkbenchBriefField::Assumptions),
        _ => None,
    }
}
