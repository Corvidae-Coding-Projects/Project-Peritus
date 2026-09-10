//! Queue commands select exact inspected revisions, preserving the composer until acceptance.

use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchIntent,
    WorkbenchMode,
};
use crate::model::decode_hex_16;
use peritus_app_protocol::{
    MAX_WORKBENCH_INPUT_PAGE, WellKnownProtocolFeature, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputSelection, WorkbenchInputText, WorkbenchNewInput, WorkbenchQueueIntent,
    WorkbenchQueuePage, WorkbenchQueueQuery,
};

impl AppModel {
    pub(in crate::model::chat) fn queue_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.queue_available() {
            self.notice(NoticeLevel::Warning, "Durable input queue unavailable/offline; /reconnect or upgrade daemon. Draft retained.");
            return Vec::new();
        }
        if self.chat.workbench.selected.is_none() {
            self.notice(
                NoticeLevel::Warning,
                "Select a conversation with /sessions first; draft retained.",
            );
            return Vec::new();
        }
        let (action, text) = split(arguments);
        self.chat.workbench.mode = WorkbenchMode::Queue;
        self.chat.workbench.context_mode = None;
        self.chat.workbench.images.open = false;
        self.chat.workbench.open = true;
        if action == "retry" && text.is_empty() {
            return self.retry_workbench();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            self.notice(NoticeLevel::Warning, "Resolve the pending control receipt before another queue operation. Draft retained.");
            return Vec::new();
        }
        if text.is_empty() && matches!(action, "" | "pending" | "history") {
            self.chat.workbench.queue_detail = None;
            return self.refresh_queue(0, 0, action == "history");
        }
        if action == "show" {
            match self.queue_selection(text) {
                Ok(selected) => {
                    self.chat.workbench.queue_detail = Some(selected);
                    self.chat.workbench.scroll = 0;
                }
                Err(message) => self.notice(NoticeLevel::Warning, message),
            }
            return Vec::new();
        }
        if text.is_empty() && matches!(action, "next" | "previous") {
            let Ok(page_size) = u32::try_from(MAX_WORKBENCH_INPUT_PAGE) else { return Vec::new() };
            let Some(page) = &self.chat.workbench.queue else {
                return self.refresh_queue(0, 0, false);
            };
            let offset = if action == "next" {
                let next = page.query().offset().saturating_add(page_size);
                if next >= page.total() {
                    return Vec::new();
                }
                next
            } else {
                page.query().offset().saturating_sub(page_size)
            };
            return self.refresh_queue(page.query().revision(), offset, page.query().history());
        }
        let intent = match self.queue_intent(action, text) {
            Ok(intent) => intent,
            Err(message) => {
                self.notice(NoticeLevel::Warning, message);
                return Vec::new();
            }
        };
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        self.submit_workbench(WorkbenchIntent::Queue(intent), query.workspace())
    }

    fn queue_intent(
        &mut self,
        action: &str,
        text: &str,
    ) -> Result<WorkbenchQueueIntent, &'static str> {
        let bad_text = "Input must be 1–8192 bytes without terminal controls; draft retained.";
        let input_text =
            |text: &str| WorkbenchInputText::new(text.to_owned()).map_err(|_| bad_text);
        if action == "add" {
            let id =
                WorkbenchInputId::new(self.ids.bytes(b"workbench-input")).map_err(|_| bad_text)?;
            return WorkbenchNewInput::new(
                id,
                input_text(text)?,
                WorkbenchInputOrder::new(Vec::new()).map_err(|_| bad_text)?,
            )
            .map(WorkbenchQueueIntent::Enqueue)
            .map_err(|_| bad_text);
        }
        if action == "order" {
            let order = text.split_whitespace().map(|value| decode_hex_16(value)
                .and_then(|bytes| WorkbenchInputId::new(bytes).ok())
                .ok_or("Use /queue order <all pending 32-hex IDs, in desired order>; draft retained."))
                .collect::<Result<Vec<_>, _>>()?;
            return WorkbenchInputOrder::new(order).map(WorkbenchQueueIntent::Reorder).map_err(
                |_| "Order contains duplicate IDs or exceeds the queue bound; draft retained.",
            );
        }
        if !matches!(action, "edit" | "correct" | "hold" | "release" | "withdraw") {
            return Err(
                "Use /queue [add <text> | edit <row> <text> | correct <row> <text> | hold <row> | release <row> | withdraw <row> | order <IDs> | history | pending | next | previous | retry]. Draft retained.",
            );
        }
        let (row, replacement) = split(text);
        let selected = self.queue_selection(row)?;
        match action {
            "edit" => Ok(WorkbenchQueueIntent::Edit { selected, text: input_text(replacement)? }),
            "correct" => Ok(WorkbenchQueueIntent::Correct {
                original: selected,
                id: WorkbenchInputId::new(self.ids.bytes(b"workbench-correction"))
                    .map_err(|_| bad_text)?,
                text: input_text(replacement)?,
            }),
            "hold" | "release" if replacement.is_empty() => {
                Ok(WorkbenchQueueIntent::Hold { selected, held: action == "hold" })
            }
            "withdraw" if replacement.is_empty() => Ok(WorkbenchQueueIntent::Withdraw(selected)),
            _ => Err("This queue action takes only one inspected row number; draft retained."),
        }
    }

    fn queue_selection(&self, text: &str) -> Result<WorkbenchInputSelection, &'static str> {
        let page = self
            .chat
            .workbench
            .queue
            .as_ref()
            .ok_or("Open /queue and inspect the exact row first; draft retained.")?;
        let index = text
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| index.checked_sub(page.query().offset() as usize));
        index
            .and_then(|index| page.rows().get(index))
            .map(peritus_app_protocol::WorkbenchInputRow::selected)
            .ok_or("Choose a row number from the currently inspected queue page; draft retained.")
    }

    pub(super) fn refresh_queue(
        &mut self,
        revision: u64,
        offset: u32,
        history: bool,
    ) -> Vec<Effect> {
        if !self.queue_available() || self.workbench_request_pending() {
            return Vec::new();
        }
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        let Ok(query) = WorkbenchQueueQuery::new(query, revision, offset, history) else {
            return Vec::new();
        };
        self.request(
            AppRequestPayload::QueryWorkbenchQueue(query),
            PendingRequest::WorkbenchQueue(query),
        )
        .into_iter()
        .collect()
    }

    pub(in crate::model) fn accept_workbench_queue(
        &mut self,
        query: WorkbenchQueueQuery,
        page: WorkbenchQueuePage,
    ) {
        if page.query().query() != query.query()
            || self.chat.workbench.selected != Some(query.query())
            || (query.revision() != 0 && page.query().revision() != query.revision())
            || page.query().offset() != query.offset()
            || page.query().history() != query.history()
        {
            return;
        }
        self.chat.workbench.queue = Some(page);
        self.chat.workbench.queue_detail = None;
        self.chat.workbench.scroll = 0;
    }
    fn queue_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchInputs.as_str()
            })
    }
}
fn split(value: &str) -> (&str, &str) {
    value.split_once(char::is_whitespace).map_or((value, ""), |(first, rest)| (first, rest.trim()))
}
