//! Read-only context commands preserve drafts and select exact revision-fenced views.

use super::{AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchMode};
use crate::model::decode_hex_16;
use peritus_app_protocol::{
    MAX_WORKBENCH_CONTEXT_PAGE, WellKnownProtocolFeature, WorkbenchContextPage,
    WorkbenchContextPreference as P, WorkbenchContextQuery, WorkbenchContextSource as S,
    WorkbenchContextView as V, WorkbenchIntent, WorkbenchInvocationId,
};

impl AppModel {
    pub(in crate::model::chat) fn context_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.context_available() {
            self.notice(NoticeLevel::Warning, "Context inspection unavailable/offline; /reconnect or upgrade daemon. Draft retained.");
            return Vec::new();
        }
        if self.chat.workbench.selected.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first; draft retained.",
            );
            return Vec::new();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(
                NoticeLevel::Warning,
                "Wait for the pending workbench request; draft retained.",
            );
            return Vec::new();
        }
        let (action, text) = arguments
            .split_once(char::is_whitespace)
            .map_or((arguments, ""), |(a, b)| (a, b.trim()));
        if matches!(action, "pin" | "exclude" | "default") {
            return self.context_preference_command(action, text);
        }
        let view = match (action, text) {
            ("" | "next", "") => V::Next,
            ("history", "") => V::History,
            ("show", id) => {
                let Some(id) = decode_hex_16(id).and_then(|id| WorkbenchInvocationId::new(id).ok())
                else {
                    self.notice(
                        NoticeLevel::Warning,
                        "Use /context show <32 hexadecimal invocation ID>; draft retained.",
                    );
                    return Vec::new();
                };
                V::Invocation(id)
            }
            ("more" | "previous", "") => return self.context_page_command(action == "more"),
            _ => {
                self.notice(NoticeLevel::Warning, "Use /context [next | history | show <invocation ID> | more | previous | pin <row> | exclude <row> | default <row>]; draft retained.");
                return Vec::new();
            }
        };
        self.chat.workbench.context_mode = Some(view);
        self.chat.workbench.mode = WorkbenchMode::Sessions;
        self.chat.workbench.images.open = false;
        self.chat.workbench.context_page = None;
        self.chat.workbench.open = true;
        "Reading local context metadata; no inference or recovery."
            .clone_into(&mut self.chat.workbench.message);
        self.refresh_context(0, 0, view)
    }

    fn context_page_command(&mut self, forward: bool) -> Vec<Effect> {
        let Some(page) = self
            .chat
            .workbench
            .context_page
            .as_ref()
            .filter(|page| self.chat.workbench.context_mode == Some(page.query().view()))
        else {
            return Vec::new();
        };
        let size = u32::try_from(MAX_WORKBENCH_CONTEXT_PAGE).unwrap_or(u32::MAX);
        let offset = if forward {
            let next = page.query().offset().saturating_add(size);
            if next >= page.total() {
                return Vec::new();
            }
            next
        } else {
            page.query().offset().saturating_sub(size)
        };
        let query = page.query();
        self.chat.workbench.open = true;
        self.refresh_context(query.revision(), offset, query.view())
    }

    fn context_preference_command(&mut self, action: &str, row: &str) -> Vec<Effect> {
        let Some(page) = self.chat.workbench.context_page.as_ref().filter(|page| {
            self.chat.workbench.context_mode == Some(V::Next) && page.query().view() == V::Next
        }) else {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /context next before changing a source preference; draft retained.",
            );
            return Vec::new();
        };
        let Some(index) = row
            .parse::<u32>()
            .ok()
            .and_then(|number| number.checked_sub(page.query().offset() + 1))
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < page.rows().len())
        else {
            self.notice(
                NoticeLevel::Warning,
                "Select an exact numbered row from the current /context next page; draft retained.",
            );
            return Vec::new();
        };
        let source = page.rows()[index].source();
        let preference = match action {
            "pin" => Some(P::Pinned),
            "exclude" => Some(P::Excluded),
            "default" => None,
            _ => return Vec::new(),
        };
        if matches!((source, preference), (S::Input(_), Some(P::Excluded))) {
            self.notice(
                NoticeLevel::Warning,
                "User inputs cannot be excluded here; hold or withdraw them in /queue. Draft retained.",
            );
            return Vec::new();
        }
        let workspace = page.query().query().workspace();
        self.submit_workbench(WorkbenchIntent::SetContext { source, preference }, workspace)
    }

    pub(super) fn refresh_context(&mut self, revision: u64, offset: u32, view: V) -> Vec<Effect> {
        if !self.context_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(scope) = self.chat.workbench.selected else {
            return Vec::new();
        };
        let Ok(query) = WorkbenchContextQuery::new(scope, revision, offset, view) else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryWorkbenchContext(query),
            PendingRequest::WorkbenchContext(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_context(
        &mut self,
        query: WorkbenchContextQuery,
        page: WorkbenchContextPage,
    ) {
        if page.query().query() != query.query()
            || self.chat.workbench.selected != Some(query.query())
            || (query.revision() != 0 && page.query().revision() != query.revision())
            || page.query().offset() != query.offset()
            || page.query().view() != query.view()
            || self.chat.workbench.context_mode != Some(query.view())
        {
            return;
        }
        self.chat.workbench.context_page = Some(page);
        self.chat.workbench.scroll = 0;
        self.chat.workbench.message.clear();
    }

    fn context_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchContext.as_str()
            })
    }
}
