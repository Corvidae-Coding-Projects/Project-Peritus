//! Negotiated metadata and exact UTF-8 body navigation at one inspected revision.
use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use peritus_app_protocol::{
    ControlOperationId, WORKBENCH_BRIEF_PAGE_ITEMS, WellKnownProtocolFeature, WorkbenchBrief,
    WorkbenchBriefField, WorkbenchBriefPage, WorkbenchBriefPageRequest, WorkbenchBriefProposalPage,
    WorkbenchBriefProposalRequest, WorkbenchIntent,
};
impl AppModel {
    pub(super) fn brief_pages_available(&self) -> bool {
        self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::WorkbenchBriefPages.as_str()
        })
    }
    pub(super) fn request_brief_page(&mut self, request: WorkbenchBriefPageRequest) -> Vec<Effect> {
        self.request(
            AppRequestPayload::QueryWorkbenchBriefPage(request),
            PendingRequest::WorkbenchBriefPage(request),
        )
        .into_iter()
        .collect()
    }
    pub(super) fn navigate_brief_pages(&mut self, action: &str, remainder: &str) -> Vec<Effect> {
        let Some(page) = self
            .chat
            .workbench
            .brief_page
            .as_ref()
            .filter(|page| Some(page.request().query()) == self.chat.workbench.selected)
        else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /brief before navigating proposals; draft retained.",
            );
            return Vec::new();
        };
        if matches!(action, "next" | "previous") {
            let advance = |offset: u64, total: u64| {
                if action == "next" {
                    let next = offset.saturating_add(WORKBENCH_BRIEF_PAGE_ITEMS as u64);
                    if next < total { next } else { offset }
                } else {
                    offset.saturating_sub(WORKBENCH_BRIEF_PAGE_ITEMS as u64)
                }
            };
            let request = WorkbenchBriefPageRequest::new(
                page.request().query(),
                page.revision(),
                advance(page.request().proposals(), page.proposal_total()),
                advance(page.request().observations(), page.observation_total()),
            )
            .expect("inspected revision");
            self.chat.workbench.open = true;
            self.chat.workbench.mode = WorkbenchMode::Brief;
            return self.request_brief_page(request);
        }
        let request = if action == "show" {
            let id = crate::model::decode_hex_16(remainder)
                .and_then(|bytes| ControlOperationId::new(bytes).ok());
            let Some(reference) =
                page.proposals().iter().find(|proposal| Some(proposal.operation()) == id)
            else {
                self.notice(
                    NoticeLevel::Warning,
                    "Use /brief show <exact proposal ID on this page>.",
                );
                return Vec::new();
            };
            WorkbenchBriefProposalRequest::new(
                page.request().query(),
                page.revision(),
                *reference,
                0,
            )
            .expect("inspected nonempty proposal")
        } else {
            let Some(body) = self.chat.workbench.brief_body.as_ref() else { return Vec::new() };
            let offset = if remainder == "next" {
                body.next()
            } else if remainder == "previous" {
                self.chat
                    .workbench
                    .brief_body_history
                    .iter()
                    .copied()
                    .filter(|offset| *offset < body.request().offset())
                    .max()
            } else {
                None
            };
            let Some(offset) = offset else { return Vec::new() };
            WorkbenchBriefProposalRequest::new(
                body.request().query(),
                body.request().revision(),
                body.request().proposal(),
                offset,
            )
            .expect("inspected text boundary")
        };
        self.chat.workbench.open = true;
        self.chat.workbench.mode = WorkbenchMode::Brief;
        self.request(
            AppRequestPayload::QueryWorkbenchBriefProposal(request),
            PendingRequest::WorkbenchBriefProposal(request),
        )
        .into_iter()
        .collect()
    }
    pub(super) fn accept_paged_proposal(
        &mut self,
        field: WorkbenchBriefField,
        operation: ControlOperationId,
    ) -> Vec<Effect> {
        let Some(body) = self.chat.workbench.brief_body.as_ref().filter(|body| {
            body.request().proposal().operation() == operation
                && Some(body.request().query()) == self.chat.workbench.selected
        }) else {
            self.notice(NoticeLevel::Warning, "Open the exact proposal with /brief show <ID> before accepting its complete immutable text.");
            return Vec::new();
        };
        self.submit_workbench(
            WorkbenchIntent::AcceptBriefProposal {
                field,
                proposal: operation,
                digest: body.request().proposal().digest(),
            },
            body.request().query().workspace(),
        )
    }
    pub(in crate::model) fn accept_brief_page(
        &mut self,
        request: WorkbenchBriefPageRequest,
        page: WorkbenchBriefPage,
    ) -> Vec<Effect> {
        if page.request() != request
            || Some(request.query()) != self.chat.workbench.selected
            || (self.chat.workbench.mode != WorkbenchMode::Brief && !self.chat.workbench.goal_mode)
        {
            return Vec::new();
        }
        let brief = WorkbenchBrief::new(request.query(), page.revision(), page.entries().to_vec())
            .expect("validated page entries");
        self.chat.workbench.brief_page = Some(page);
        self.chat.workbench.brief_body = None;
        self.chat.workbench.brief_body_history.clear();
        self.accept_workbench_brief(request.query(), brief)
    }
    pub(in crate::model) fn accept_brief_body(
        &mut self,
        request: WorkbenchBriefProposalRequest,
        body: WorkbenchBriefProposalPage,
    ) {
        if body.request() != request
            || Some(request.query()) != self.chat.workbench.selected
            || self.chat.workbench.mode != WorkbenchMode::Brief
            || self
                .chat
                .workbench
                .brief_page
                .as_ref()
                .is_none_or(|page| page.revision() != request.revision())
        {
            return;
        }
        if self
            .chat
            .workbench
            .brief_body
            .as_ref()
            .is_none_or(|old| old.request().proposal() != request.proposal())
        {
            self.chat.workbench.brief_body_history.clear();
        }
        if !self.chat.workbench.brief_body_history.contains(&request.offset()) {
            self.chat.workbench.brief_body_history.push(request.offset());
        }
        self.chat.workbench.brief_body = Some(body);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
        self.complete_workbench_inspection();
    }
}
