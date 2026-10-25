//! Queue commands select exact inspected revisions, preserving the composer until acceptance.

use super::{
    AppModel, AppRequestPayload, Effect, NoticeLevel, PendingRequest, WorkbenchIntent,
    WorkbenchMode,
};
use crate::model::decode_hex_16;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    MAX_WORKBENCH_INPUT_PAGE, WellKnownProtocolFeature, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputSelection, WorkbenchInputState, WorkbenchInputText, WorkbenchNewInput,
    WorkbenchQuery, WorkbenchQueueIntent, WorkbenchQueuePage, WorkbenchQueueQuery,
};

/// Local keyboard selection; identity and content revision never follow a changing row number.
#[derive(Clone, Copy, Debug)]
pub struct QueueTarget {
    pub(crate) query: WorkbenchQuery,
    pub(crate) revision: u64,
    pub(crate) selected: WorkbenchInputSelection,
}

/// A separate multiline draft. Saving is a typed queue operation, never model input.
#[derive(Debug)]
pub struct QueueEditor {
    pub(crate) target: QueueTarget,
    pub(crate) text: String,
    pub(crate) cursor: usize,
    pub(crate) anchor: Option<usize>,
}

impl AppModel {
    pub(in crate::model::chat) fn queue_command(&mut self, arguments: &str) -> Vec<Effect> {
        if !self.queue_available() {
            self.notice(NoticeLevel::Warning, "Durable input queue unavailable/offline; /reconnect or upgrade daemon. Draft retained.");
            return Vec::new();
        }
        if self.chat.workbench.selected.is_none() {
            return self
                .create_command_conversation("New conversation", format!("/queue {arguments}"));
        }
        let (action, text) = split(arguments);
        self.chat.workbench.mode = WorkbenchMode::Queue;
        self.chat.workbench.files.open = false;
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
        self.chat.workbench.message.clear();
        if action == "start" && text.is_empty() {
            return self.start_pending_workbench();
        }
        if text.is_empty() && matches!(action, "" | "pending" | "history") {
            self.chat.workbench.queue_detail = None;
            self.chat.workbench.queue_selected = 0;
            return self.refresh_queue(0, 0, action == "history");
        }
        if action == "show" {
            match self.queue_selection(text) {
                Ok(selected) => {
                    self.chat.workbench.queue_selected = self
                        .chat
                        .workbench
                        .queue
                        .as_ref()
                        .and_then(|page| {
                            page.rows().iter().position(|row| row.selected() == selected)
                        })
                        .unwrap_or(0);
                    self.chat.workbench.queue_detail = Some(selected);
                    self.chat.workbench.scroll = 0;
                    self.clear_chat_command();
                }
                Err(message) => self.notice(NoticeLevel::Warning, message),
            }
            return Vec::new();
        }
        if let Some(effects) = self.queue_page_command(action, text) {
            return effects;
        }
        if action == "add" && text.len() > peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES {
            return self.send_workbench_text(text.to_owned(), true);
        }
        let intent = match self.queue_intent(action, text) {
            Ok(intent) => intent,
            Err(message) => {
                self.notice(NoticeLevel::Warning, message);
                return Vec::new();
            }
        };
        let Some(query) = self.chat.workbench.selected else { return Vec::new() };
        if matches!(intent, WorkbenchQueueIntent::Move { .. } | WorkbenchQueueIntent::Reorder(_)) {
            let Some(page) =
                self.chat.workbench.queue.as_ref().filter(|page| !page.query().history())
            else {
                self.notice(
                    NoticeLevel::Warning,
                    "Inspect the pending queue before changing its order; draft retained.",
                );
                return Vec::new();
            };
            return self.submit_bound_workbench(
                WorkbenchIntent::Queue(intent),
                query,
                page.query().revision(),
            );
        }
        // Resolve row numbers once, against the page the user inspected. Refresh only the
        // aggregate fence; never reinterpret the user's row against a reordered queue.
        self.request(
            AppRequestPayload::QueryWorkbench(query),
            PendingRequest::WorkbenchQueueCommand {
                query,
                intent,
                draft: self.chat.buffer.clone(),
            },
        )
        .into_iter()
        .collect()
    }

    fn queue_page_command(&mut self, action: &str, text: &str) -> Option<Vec<Effect>> {
        if !text.is_empty() || !matches!(action, "next" | "previous") {
            return None;
        }
        let Ok(page_size) = u32::try_from(MAX_WORKBENCH_INPUT_PAGE) else {
            return Some(Vec::new());
        };
        let Some(page) = &self.chat.workbench.queue else {
            return Some(self.refresh_queue(0, 0, false));
        };
        let offset = if action == "next" {
            let next = page.query().offset().saturating_add(page_size);
            if next >= page.total() {
                self.clear_chat_command();
                return Some(Vec::new());
            }
            next
        } else {
            page.query().offset().saturating_sub(page_size)
        };
        Some(self.refresh_queue(page.query().revision(), offset, page.query().history()))
    }

    pub(in crate::model) fn accept_queue_command_snapshot(
        &mut self,
        query: WorkbenchQuery,
        intent: WorkbenchQueueIntent,
        draft: &str,
        snapshot: &peritus_app_protocol::WorkbenchSnapshot,
    ) -> Vec<Effect> {
        if snapshot.query() != query
            || self.chat.workbench.selected != Some(query)
            || self.chat.workbench.mode != WorkbenchMode::Queue
            || self.chat.buffer != draft
            || !self.queue_available()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
        {
            return Vec::new();
        }
        self.chat.workbench.inspection_draft = None;
        self.chat.workbench.snapshot = Some(snapshot.clone());
        self.submit_bound_workbench(WorkbenchIntent::Queue(intent), query, snapshot.revision())
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
        if action == "move" {
            if !self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchQueueMoves.as_str()
            }) {
                return Err(
                    "This daemon does not support incremental ordering; upgrade/reconnect. Draft retained.",
                );
            }
            let (source, target) = split(text);
            let selected = self.queue_selection(source)?;
            let before = if target == "end" {
                None
            } else {
                let (keyword, id) = split(target);
                if keyword != "before" {
                    return Err(
                        "Use /queue move <inspected row or ID> before <32-hex ID>, or end; draft retained.",
                    );
                }
                Some(
                    decode_hex_16(id)
                        .and_then(|bytes| WorkbenchInputId::new(bytes).ok())
                        .ok_or("Destination must be an exact pending 32-hex ID; draft retained.")?,
                )
            };
            return Ok(WorkbenchQueueIntent::Move { selected, before });
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
                "Use /queue [start | add <text> | edit <row> <text> | correct <row> <text> | hold <row> | release <row> | withdraw <row> | order <IDs> | move <row/ID> before <ID>/end | history | pending | next | previous | retry]. Draft retained.",
            );
        }
        let (row, replacement) = split(text);
        let selected = self.queue_selection(row)?;
        let state = self
            .chat
            .workbench
            .queue
            .as_ref()
            .and_then(|page| page.rows().iter().find(|row| row.selected() == selected))
            .map(peritus_app_protocol::WorkbenchInputRow::state);
        if action == "correct" && state != Some(WorkbenchInputState::Incorporated) {
            return Err(
                "This input has not been incorporated. Use /queue edit <row> <text> to change a pending input; corrections apply to incorporated history. Draft retained.",
            );
        }
        if action == "edit"
            && !matches!(state, Some(WorkbenchInputState::Queued | WorkbenchInputState::Held))
        {
            return Err(
                "This revision is no longer pending. Use /queue correct <row> <text> for incorporated history. Draft retained.",
            );
        }
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
        if let Some(id) = decode_hex_16(text).and_then(|bytes| WorkbenchInputId::new(bytes).ok()) {
            return page
                .rows()
                .iter()
                .find(|row| row.selected().id() == id)
                .map(peritus_app_protocol::WorkbenchInputRow::selected)
                .ok_or("Inspect the page containing that exact input ID first; draft retained.");
        }
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
        let selected = self.chat.workbench.queue.as_ref().and_then(|old| {
            old.rows()
                .get(self.chat.workbench.queue_selected)
                .map(peritus_app_protocol::WorkbenchInputRow::selected)
        });
        self.chat.workbench.queue_selected = selected
            .and_then(|selected| {
                page.rows().iter().position(|row| row.selected() == selected).or_else(|| {
                    page.rows().iter().position(|row| row.selected().id() == selected.id())
                })
            })
            .unwrap_or_else(|| {
                self.chat.workbench.queue_selected.min(page.rows().len().saturating_sub(1))
            });
        self.chat.workbench.queue = Some(page);
        self.complete_workbench_inspection();
        self.chat.workbench.queue_detail = None;
        self.chat.workbench.scroll = 0;
    }
    fn queue_available(&self) -> bool {
        self.workbench_available()
            && self.features.iter().any(|feature| {
                feature.as_str() == WellKnownProtocolFeature::WorkbenchInputs.as_str()
            })
    }

    pub(in crate::model::chat::workbench) fn queue_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<Vec<Effect>> {
        if self.chat.workbench.queue_editor.is_some() {
            return Some(self.edit_queue_key(key));
        }
        if let Some(target) = self.chat.workbench.queue_withdraw {
            return match key.code {
                KeyCode::Esc => {
                    self.chat.workbench.queue_withdraw = None;
                    Some(Vec::new())
                }
                KeyCode::Enter => {
                    let effects = self.submit_queue_keyboard(
                        WorkbenchQueueIntent::Withdraw(target.selected),
                        target,
                    );
                    if !effects.is_empty() {
                        self.chat.workbench.queue_withdraw = None;
                    }
                    Some(effects)
                }
                _ => Some(Vec::new()),
            };
        }
        match key.code {
            KeyCode::Up | KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                return Some(self.move_queue_keyboard(key.code == KeyCode::Down));
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
                if let Some(page) = &self.chat.workbench.queue {
                    let last = page.rows().len().saturating_sub(1);
                    self.chat.workbench.queue_selected = match key.code {
                        KeyCode::Up => self.chat.workbench.queue_selected.saturating_sub(1),
                        KeyCode::Down => {
                            self.chat.workbench.queue_selected.saturating_add(1).min(last)
                        }
                        KeyCode::Home => 0,
                        _ => last,
                    };
                    self.chat.workbench.queue_detail = None;
                    self.chat.workbench.scroll = 0;
                }
                return Some(Vec::new());
            }
            KeyCode::Enter => {
                self.chat.workbench.queue_detail =
                    self.queue_keyboard_target().map(|target| target.selected);
                self.chat.workbench.scroll = 0;
                return Some(Vec::new());
            }
            KeyCode::Esc if self.chat.workbench.queue_detail.is_some() => {
                self.chat.workbench.queue_detail = None;
                self.chat.workbench.scroll = 0;
                return Some(Vec::new());
            }
            KeyCode::Char('n' | 'p') if key.modifiers.is_empty() => {
                return Some(self.page_queue_keyboard(key.code == KeyCode::Char('n')));
            }
            KeyCode::Char('e' | 'h') | KeyCode::Delete if key.modifiers.is_empty() => {
                if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
                    self.notice(
                        NoticeLevel::Warning,
                        "Wait for the current queue operation's receipt. Both drafts are retained.",
                    );
                    return Some(Vec::new());
                }
                let Some(target) = self.queue_pending_target() else { return Some(Vec::new()) };
                let row = self
                    .chat
                    .workbench
                    .queue
                    .as_ref()?
                    .rows()
                    .get(self.chat.workbench.queue_selected)?;
                match key.code {
                    KeyCode::Char('e') => {
                        let text = row.text().as_str().to_owned();
                        self.chat.workbench.queue_editor =
                            Some(QueueEditor { target, cursor: text.len(), text, anchor: None });
                    }
                    KeyCode::Char('h') => {
                        let held = row.state() != WorkbenchInputState::Held;
                        return Some(self.submit_queue_keyboard(
                            WorkbenchQueueIntent::Hold { selected: target.selected, held },
                            target,
                        ));
                    }
                    _ => self.chat.workbench.queue_withdraw = Some(target),
                }
                return Some(Vec::new());
            }
            _ => {}
        }
        None
    }

    fn queue_keyboard_target(&self) -> Option<QueueTarget> {
        let page = self.chat.workbench.queue.as_ref()?;
        Some(QueueTarget {
            query: page.query().query(),
            revision: page.query().revision(),
            selected: page.rows().get(self.chat.workbench.queue_selected)?.selected(),
        })
    }

    fn queue_pending_target(&mut self) -> Option<QueueTarget> {
        let target = self.queue_keyboard_target()?;
        let pending = self.chat.workbench.queue.as_ref().is_some_and(|page| {
            page.rows().iter().any(|row| {
                row.selected() == target.selected
                    && matches!(
                        row.state(),
                        WorkbenchInputState::Queued | WorkbenchInputState::Held
                    )
            })
        });
        if !pending {
            self.notice(NoticeLevel::Warning, "Only pending queued or held inputs can be changed. Incorporated history stays immutable.");
        }
        pending.then_some(target)
    }

    fn submit_queue_keyboard(
        &mut self,
        intent: WorkbenchQueueIntent,
        target: QueueTarget,
    ) -> Vec<Effect> {
        if !self.queue_available()
            || self.workbench_request_pending()
            || self.chat.workbench.unresolved.is_some()
            || self.chat.workbench.selected != Some(target.query)
        {
            self.notice(
                NoticeLevel::Warning,
                "Queue operation unavailable or awaiting a receipt; drafts retained.",
            );
            return Vec::new();
        }
        let effects = self.submit_bound_workbench(
            WorkbenchIntent::Queue(intent),
            target.query,
            target.revision,
        );
        if !effects.is_empty() {
            self.chat.workbench.queue_keyboard_operation =
                self.chat.workbench.unresolved.as_ref().map(|(command, _)| command.operation());
        }
        effects
    }

    fn move_queue_keyboard(&mut self, down: bool) -> Vec<Effect> {
        if !self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::WorkbenchQueueMoves.as_str()
        }) {
            self.notice(
                NoticeLevel::Warning,
                "This daemon does not support moving queue inputs; upgrade or reconnect.",
            );
            return Vec::new();
        }
        let Some(target) = self.queue_pending_target() else { return Vec::new() };
        let Some(page) = &self.chat.workbench.queue else { return Vec::new() };
        if page.query().history() {
            self.notice(
                NoticeLevel::Warning,
                "Inspect /queue pending before moving inputs in their current pending order.",
            );
            return Vec::new();
        }
        let index = self.chat.workbench.queue_selected;
        let before = if down {
            if index + 1 >= page.rows().len() {
                if page.query().offset() as usize + index + 1 < page.total() as usize {
                    self.notice(NoticeLevel::Warning, "The next input is on another page. Inspect it before choosing an exact move destination.");
                }
                return Vec::new();
            }
            match page.rows().get(index + 2) {
                Some(row) => Some(row.selected().id()),
                None if page.query().offset() as usize + page.rows().len()
                    == page.total() as usize =>
                {
                    None
                }
                None => {
                    self.notice(NoticeLevel::Warning, "The following move destination is on another page; inspect it before moving.");
                    return Vec::new();
                }
            }
        } else {
            let Some(previous) =
                index.checked_sub(1).and_then(|previous| page.rows().get(previous))
            else {
                return Vec::new();
            };
            Some(previous.selected().id())
        };
        self.submit_queue_keyboard(
            WorkbenchQueueIntent::Move { selected: target.selected, before },
            target,
        )
    }

    fn page_queue_keyboard(&mut self, next: bool) -> Vec<Effect> {
        let Some(page) = &self.chat.workbench.queue else {
            return self.refresh_queue(0, 0, false);
        };
        let size = u32::try_from(MAX_WORKBENCH_INPUT_PAGE).unwrap_or(u32::MAX);
        let offset = if next {
            page.query().offset().saturating_add(size)
        } else {
            page.query().offset().saturating_sub(size)
        };
        if (next && offset >= page.total()) || offset == page.query().offset() {
            return Vec::new();
        }
        let (revision, history) = (page.query().revision(), page.query().history());
        self.chat.workbench.queue_selected = 0;
        self.chat.workbench.queue_detail = None;
        self.refresh_queue(revision, offset, history)
    }

    fn edit_queue_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.code == KeyCode::Esc {
            self.chat.workbench.queue_editor = None;
            return Vec::new();
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            return Vec::new();
        }
        if key.code == KeyCode::Enter {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                self.paste_queue_editor("\n");
                return Vec::new();
            }
            return self.save_queue_editor();
        }
        let width = self.chat.viewport.map_or(78, |area| usize::from(area.width.saturating_sub(2)));
        let Some(editor) = self.chat.workbench.queue_editor.as_mut() else { return Vec::new() };
        if let Some(down) = crate::input::vertical_direction(key) {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                editor.anchor.get_or_insert(editor.cursor);
            } else {
                editor.anchor = None;
            }
            editor.cursor =
                crate::input::composer::vertical_cursor(&editor.text, editor.cursor, down, width);
            return Vec::new();
        }
        if let KeyCode::Char(character) = key.code
            && crate::input::text_motion(key).is_none()
            && crate::input::text_deletion(key).is_none()
            && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
            && (character.is_control()
                || editor
                    .text
                    .len()
                    .saturating_sub(
                        crate::input::selection::range(editor.anchor, editor.cursor)
                            .map_or(0, |range| range.len()),
                    )
                    .saturating_add(character.len_utf8())
                    > peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES)
        {
            return Vec::new();
        }
        crate::input::selection::edit(
            &mut editor.text,
            &mut editor.cursor,
            &mut editor.anchor,
            key,
        );
        Vec::new()
    }

    fn save_queue_editor(&mut self) -> Vec<Effect> {
        let Some(editor) = self.chat.workbench.queue_editor.as_ref() else { return Vec::new() };
        let selected = editor.target.selected;
        let query = editor.target.query;
        let Ok(text) = WorkbenchInputText::new(editor.text.clone()) else {
            self.notice(NoticeLevel::Warning, "Use 1–8192 bytes of nonempty input without terminal controls. Both drafts retained.");
            return Vec::new();
        };
        let Some(page) = &self.chat.workbench.queue else {
            let effects = self.refresh_queue(0, 0, false);
            "Refreshing the queue. Inspect it, then press Enter again to save this exact input revision."
                .clone_into(&mut self.chat.workbench.message);
            return effects;
        };
        if page.query().query() != query
            || !page.rows().iter().any(|row| {
                row.selected() == selected
                    && matches!(
                        row.state(),
                        WorkbenchInputState::Queued | WorkbenchInputState::Held
                    )
            })
        {
            self.notice(NoticeLevel::Warning, "The selected input revision is no longer pending on this page. Draft retained; Esc returns to inspect the current input.");
            return Vec::new();
        }
        let target = QueueTarget { query, selected, revision: page.query().revision() };
        self.submit_queue_keyboard(WorkbenchQueueIntent::Edit { selected, text }, target)
    }

    pub(in crate::model) fn paste_queue_editor(&mut self, text: &str) -> bool {
        if !self.chat.workbench.open
            || self.chat.workbench.mode != WorkbenchMode::Queue
            || self.chat.workbench.queue_editor.is_none()
        {
            return false;
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            return true;
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let Some(editor) = self.chat.workbench.queue_editor.as_mut() else { return true };
        let range = crate::input::selection::range(editor.anchor, editor.cursor)
            .unwrap_or(editor.cursor..editor.cursor);
        if text.chars().any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
            || editor.text.len().saturating_sub(range.len()).saturating_add(text.len())
                > peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES
        {
            self.notice(
                NoticeLevel::Warning,
                "Pasted queue edit is oversized or contains controls; nothing changed.",
            );
            return true;
        }
        editor.cursor = range.start + text.len();
        editor.text.replace_range(range, &text);
        editor.anchor = None;
        true
    }
}
fn split(value: &str) -> (&str, &str) {
    value.split_once(char::is_whitespace).map_or((value, ""), |(first, rest)| (first, rest.trim()))
}
