//! Scoped, read-only recall of durable user inputs. Recall never submits or revises an input.

use super::{AppModel, Effect, NoticeLevel, PendingRequest};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    AppRequestPayload, MAX_WORKBENCH_INPUT_PAGE, ProductInteractionQuery, WellKnownProtocolFeature,
    WorkbenchQueuePage, WorkbenchQueueQuery,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Draft {
    text: String,
    cursor: usize,
    pasted_command: bool,
    selection_anchor: Option<usize>,
}

impl Draft {
    fn capture(model: &AppModel) -> Self {
        Self {
            text: model.chat.buffer.clone(),
            cursor: model.chat.cursor,
            pasted_command: model.chat.pasted_command,
            selection_anchor: model.chat.selection_anchor,
        }
    }
    fn restore(self, model: &mut AppModel) {
        model.chat.buffer = self.text;
        model.chat.cursor = self.cursor.min(model.chat.buffer.len());
        model.chat.pasted_command = self.pasted_command;
        model.chat.selection_anchor = self.selection_anchor;
    }
}

#[derive(Debug)]
struct Browse {
    original: Draft,
    index: Option<u32>,
    edits: BTreeMap<u32, Draft>,
}

#[derive(Debug)]
pub(crate) struct Search {
    pub(crate) query: String,
    pub(crate) selected: usize,
    original: Draft,
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Newest,
    Index(u32),
    Search,
    Instructions,
}

#[derive(Debug, Default)]
pub(crate) struct ComposerHistory {
    pub(crate) rows: BTreeMap<u32, String>,
    pub(crate) search: Option<Search>,
    pub(crate) loading: Option<WorkbenchQueueQuery>,
    awaiting_binding: Option<peritus_types::RunId>,
    revision: u64,
    total: u32,
    stale: bool,
    browse: Option<Browse>,
    target: Option<Target>,
    pending_draft: Option<Draft>,
}

impl ComposerHistory {
    pub(crate) fn invalidate(&mut self) {
        self.stale = true;
    }
    pub(crate) fn submitted(&mut self) {
        self.invalidate();
        self.browse = None;
        self.search = None;
        self.target = None;
    }
    pub(crate) fn interrupted(&mut self) {
        self.loading = None;
        self.target = None;
        self.pending_draft = None;
        self.awaiting_binding = None;
    }
    pub(crate) fn is_loading(&self) -> bool {
        self.loading.is_some() || self.awaiting_binding.is_some()
    }
    pub(crate) fn browsing(&self) -> bool {
        self.browse.is_some()
    }
    pub(crate) fn hint(&self) -> Option<String> {
        let index = self.browse.as_ref()?.index?;
        Some(format!(
            "Recalled message {}/{} · edit then Enter sends anew · ↓ restores draft · Esc cancels",
            index + 1,
            self.total
        ))
    }
    pub(crate) fn matches(&self) -> Vec<(u32, &str)> {
        let query =
            self.search.as_ref().map_or(String::new(), |search| search.query.to_lowercase());
        self.rows
            .iter()
            .rev()
            .filter(|(_, text)| text.to_lowercase().contains(&query))
            .map(|(id, text)| (*id, text.as_str()))
            .collect()
    }
}

impl AppModel {
    pub(in crate::model) fn interrupt_composer_history(&mut self) {
        if (self.chat.history.loading.is_some() || self.chat.history.awaiting_binding.is_some())
            && self.chat.history.search.is_none()
            && self.chat.history.pending_draft.as_ref() != Some(&Draft::capture(self))
        {
            self.chat.history.browse = None;
        }
        self.chat.history.interrupted();
    }

    pub(in crate::model) fn take_deferred_history(
        &mut self,
        run: peritus_types::RunId,
    ) -> Option<ComposerHistory> {
        (self.chat.history.awaiting_binding == Some(run))
            .then(|| std::mem::take(&mut self.chat.history))
    }

    pub(in crate::model) fn resume_deferred_history(
        &mut self,
        mut history: ComposerHistory,
    ) -> Vec<Effect> {
        history.awaiting_binding = None;
        self.chat.history = history;
        if self.chat.history.search.is_none()
            && self.chat.history.pending_draft.as_ref() != Some(&Draft::capture(self))
        {
            self.chat.history.target = None;
            self.chat.history.browse = None;
            return Vec::new();
        }
        self.resolve_history()
    }

    pub(in crate::model) fn history_binding_error(&mut self, run: peritus_types::RunId) {
        if self.chat.history.awaiting_binding == Some(run) {
            self.interrupt_composer_history();
        }
    }

    pub(in crate::model) fn open_instruction_queue(&mut self) -> Vec<Effect> {
        self.chat.workbench.inspection_draft = None;
        if self.chat.workbench.selected.is_some() {
            return self.queue_command("");
        }
        if self.chat.run_id.is_none() {
            self.notice(NoticeLevel::Info, "No saved instructions yet. Send a message first.");
            return Vec::new();
        }
        self.chat.history.target = Some(Target::Instructions);
        self.query_composer_history(0, 0)
    }

    pub(in crate::model) fn recall_history(&mut self, older: bool) -> Vec<Effect> {
        if self.chat.history.loading.is_some() {
            return Vec::new();
        }
        if self.chat.history.browse.is_none() {
            if !older {
                return Vec::new();
            }
            if self.chat.history.stale {
                self.chat.history.rows.clear();
                self.chat.history.revision = 0;
                self.chat.history.stale = false;
            }
            self.chat.history.browse = Some(Browse {
                original: Draft::capture(self),
                index: None,
                edits: BTreeMap::new(),
            });
        }
        let index = self.chat.history.browse.as_ref().and_then(|browse| browse.index);
        if let Some(index) = index {
            let draft = Draft::capture(self);
            self.chat.history.browse.as_mut().expect("browsing").edits.insert(index, draft);
        }
        let target = match (index, older) {
            (None, _) => Target::Newest,
            (Some(0), true) => return Vec::new(),
            (Some(index), true) => Target::Index(index - 1),
            (Some(index), false) if index + 1 >= self.chat.history.total => {
                self.cancel_history_recall();
                return Vec::new();
            }
            (Some(index), false) => Target::Index(index + 1),
        };
        self.chat.history.target = Some(target);
        self.resolve_history()
    }

    fn resolve_history(&mut self) -> Vec<Effect> {
        let Some(target) = self.chat.history.target else { return Vec::new() };
        if matches!(target, Target::Instructions) {
            self.chat.history.target = None;
            return self.open_instruction_queue();
        }
        if self.chat.history.revision == 0 {
            return self.query_composer_history(0, 0);
        }
        let index = match target {
            Target::Newest | Target::Index(_) if self.chat.history.total == 0 => {
                self.chat.history.target = None;
                if self.chat.history.search.is_none() {
                    self.cancel_history_recall();
                    self.notice(NoticeLevel::Info, "No saved messages in this conversation.");
                }
                return Vec::new();
            }
            Target::Instructions => return Vec::new(),
            Target::Newest => self.chat.history.total - 1,
            Target::Index(index) => index,
            Target::Search => {
                let found = self.chat.history.matches().len();
                let selected =
                    self.chat.history.search.as_ref().map_or(0, |search| search.selected);
                let oldest =
                    self.chat.history.rows.first_key_value().map_or(0, |(index, _)| *index);
                if (found == 0 || selected >= found) && oldest > 0 {
                    let offset = oldest.saturating_sub(MAX_WORKBENCH_INPUT_PAGE as u32);
                    return self.query_composer_history(self.chat.history.revision, offset);
                }
                if let Some(search) = &mut self.chat.history.search {
                    search.selected = selected.min(found.saturating_sub(1));
                }
                self.chat.history.target = None;
                return Vec::new();
            }
        };
        if let Some(text) = self.chat.history.rows.get(&index).cloned() {
            let remembered = self
                .chat
                .history
                .browse
                .as_ref()
                .and_then(|browse| browse.edits.get(&index))
                .cloned();
            if let Some(draft) = remembered {
                draft.restore(self);
            } else {
                self.chat.buffer = text;
                self.chat.cursor = self.chat.buffer.len();
                self.chat.selection_anchor = None;
                // Recalled text is data, including any leading slash or attachment shorthand.
                self.chat.pasted_command = true;
            }
            if let Some(browse) = &mut self.chat.history.browse {
                browse.index = Some(index);
            }
            self.chat.history.target = None;
            Vec::new()
        } else {
            let size = MAX_WORKBENCH_INPUT_PAGE as u32;
            self.query_composer_history(self.chat.history.revision, index / size * size)
        }
    }

    fn query_composer_history(&mut self, revision: u64, offset: u32) -> Vec<Effect> {
        if self.chat.history.loading.is_some() {
            return Vec::new();
        }
        let Some(scope) = self.chat.workbench.selected else {
            if let Some(run) = self.chat.run_id {
                if self.chat.history.awaiting_binding != Some(run) {
                    self.chat.history.pending_draft = Some(Draft::capture(self));
                    self.chat.history.awaiting_binding = Some(run);
                }
                if self.features.is_empty() {
                    return Vec::new();
                }
                if self.features.iter().any(|feature| {
                    feature.as_str() == WellKnownProtocolFeature::WorkbenchRunBinding.as_str()
                }) {
                    if self.pending.values().any(|pending| matches!(pending, PendingRequest::ChatBinding { run_id, .. } if *run_id == run)) { return Vec::new(); }
                    return self
                        .request(
                            AppRequestPayload::QueryInteractionBinding(
                                ProductInteractionQuery::new(run),
                            ),
                            PendingRequest::ChatBinding { run_id: run, opening: false },
                        )
                        .into_iter()
                        .collect();
                }
            }
            self.chat.history.awaiting_binding = None;
            self.chat.history.target = None;
            self.cancel_history_recall();
            self.notice(NoticeLevel::Info, "Open a saved conversation to recall its messages.");
            return Vec::new();
        };
        if !matches!(self.connection, super::super::ConnectionStatus::Online { .. }) {
            self.notice(NoticeLevel::Warning, "History is offline; draft kept. Ctrl-R reconnects.");
            return Vec::new();
        }
        let Ok(query) = WorkbenchQueueQuery::new(scope, revision, offset, true) else {
            return Vec::new();
        };
        let effect = self.request(
            AppRequestPayload::QueryWorkbenchQueue(query),
            PendingRequest::ComposerHistory(query),
        );
        if effect.is_some() {
            self.chat.history.loading = Some(query);
            self.chat.history.pending_draft = Some(Draft::capture(self));
        }
        effect.into_iter().collect()
    }

    pub(in crate::model) fn accept_composer_history(
        &mut self,
        query: WorkbenchQueueQuery,
        page: &WorkbenchQueuePage,
    ) -> Vec<Effect> {
        if self.chat.history.loading != Some(query)
            || self.chat.workbench.selected != Some(query.query())
            || page.query().query() != query.query()
            || page.query().offset() != query.offset()
            || !page.query().history()
            || (query.revision() != 0 && page.query().revision() != query.revision())
        {
            return Vec::new();
        }
        self.chat.history.loading = None;
        self.chat.history.revision = page.query().revision();
        self.chat.history.total = page.total();
        // Navigation is an asynchronous read, never permission to replace newer typing.
        // Check before requesting another page, so its draft fence cannot silently advance.
        if self.chat.history.pending_draft.as_ref() != Some(&Draft::capture(self))
            && self.chat.history.search.is_none()
        {
            self.chat.history.target = None;
            self.chat.history.browse = None;
            return Vec::new();
        }
        if self.chat.history.target.is_some()
            && query.revision() == 0
            && page.total() as usize > MAX_WORKBENCH_INPUT_PAGE
        {
            self.chat.history.rows.clear();
            let size = MAX_WORKBENCH_INPUT_PAGE as u32;
            return self
                .query_composer_history(page.query().revision(), (page.total() - 1) / size * size);
        }
        for (offset, row) in page.rows().iter().enumerate() {
            self.chat
                .history
                .rows
                .insert(query.offset() + offset as u32, row.text().as_str().to_owned());
        }
        self.resolve_history()
    }

    pub(in crate::model) fn composer_history_error(&mut self, query: WorkbenchQueueQuery) {
        if self.chat.history.loading == Some(query) {
            if self.chat.history.search.is_none()
                && self.chat.history.pending_draft.as_ref() != Some(&Draft::capture(self))
            {
                self.chat.history.browse = None;
            }
            self.chat.history.loading = None;
            self.chat.history.target = None;
            self.chat.history.revision = 0;
            self.notice(
                NoticeLevel::Warning,
                "Message history changed or is unavailable. Draft kept; press Up/F3 to refresh.",
            );
        }
    }

    pub(in crate::model) fn cancel_history_recall(&mut self) -> bool {
        self.chat.history.target = None;
        self.chat.history.awaiting_binding = None;
        if let Some(browse) = self.chat.history.browse.take() {
            browse.original.restore(self);
            true
        } else {
            false
        }
    }

    pub(in crate::model) fn open_history_search(&mut self) -> Vec<Effect> {
        if self.chat.history.stale {
            self.chat.history.rows.clear();
            self.chat.history.revision = 0;
            self.chat.history.stale = false;
        }
        self.chat.history.search =
            Some(Search { query: String::new(), selected: 0, original: Draft::capture(self) });
        self.chat.history.target = Some(Target::Search);
        self.resolve_history()
    }

    pub(in crate::model) fn paste_history_search(&mut self, text: &str) -> Vec<Effect> {
        if let Some(search) = &mut self.chat.history.search {
            search.query.extend(text.chars().filter(|ch| !ch.is_control()));
            search.selected = 0;
        }
        self.chat.history.target = Some(Target::Search);
        self.resolve_history()
    }

    pub(in crate::model) fn history_search_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                if let Some(search) = self.chat.history.search.take() {
                    search.original.restore(self);
                }
                self.chat.history.target = None;
                self.chat.history.awaiting_binding = None;
                return Vec::new();
            }
            KeyCode::Enter => {
                let index = self.chat.history.search.as_ref().map_or(0, |search| search.selected);
                let selected = self.chat.history.matches().get(index).map(|(id, _)| *id);
                if let Some(selected) = selected {
                    let original = self.chat.history.search.take().expect("search open").original;
                    if self.chat.history.browse.is_none() {
                        self.chat.history.browse =
                            Some(Browse { original, index: None, edits: BTreeMap::new() });
                    }
                    self.chat.history.target = Some(Target::Index(selected));
                    return self.resolve_history();
                }
                return Vec::new();
            }
            KeyCode::Up => {
                if let Some(search) = &mut self.chat.history.search {
                    search.selected = search.selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::F(3) => {
                if let Some(search) = &mut self.chat.history.search {
                    search.selected = search.selected.saturating_add(1);
                }
            }
            KeyCode::Backspace => {
                if let Some(search) = &mut self.chat.history.search {
                    search.query.pop();
                    search.selected = 0;
                }
            }
            KeyCode::Char(ch)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                if let Some(search) = &mut self.chat.history.search {
                    search.query.push(ch);
                    search.selected = 0;
                }
            }
            _ => return Vec::new(),
        }
        self.chat.history.target = Some(Target::Search);
        self.resolve_history()
    }
}
