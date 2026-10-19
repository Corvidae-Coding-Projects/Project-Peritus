//! Local convenience overlays preserve the main draft and never submit inference.

use super::{AppModel, Effect, NoticeLevel};
use crate::{
    highlight::Theme,
    local::{LocalOperation, LocalReply},
    preferences::{Preferences, Shortcut},
    transcript_archive::SearchPage,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Panel {
    Find,
    Files,
    Keys,
    Theme,
    Code,
}

#[derive(Debug)]
struct PendingLocal {
    generation: u64,
    query: String,
    draft: String,
    target: (Option<peritus_types::RunId>, Option<peritus_app_protocol::WorkbenchQuery>),
}

#[derive(Debug)]
pub(crate) struct Convenience {
    pub(crate) preferences: Preferences,
    pub(crate) panel: Option<Panel>,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) file_root: Option<PathBuf>,
    pub(crate) search: Option<SearchPage>,
    pub(crate) preview: Option<String>,
    pub(crate) blocks: Vec<(String, String)>,
    pub(crate) error: String,
    pub(crate) capturing: bool,
    pub(crate) loading: bool,
    result_query: Option<String>,
    generation: u64,
    pending: Option<PendingLocal>,
}

impl Default for Convenience {
    fn default() -> Self {
        let (preferences, error) = match Preferences::load() {
            Ok(p) => (p, String::new()),
            Err(e) => (Preferences::default(), e),
        };
        Self {
            preferences,
            error,
            panel: None,
            query: String::new(),
            selected: 0,
            scroll: 0,
            files: Vec::new(),
            file_root: None,
            search: None,
            preview: None,
            blocks: Vec::new(),
            capturing: false,
            loading: false,
            result_query: None,
            generation: 0,
            pending: None,
        }
    }
}

impl AppModel {
    pub(in crate::model) fn refresh_convenience_search(&mut self) -> Vec<Effect> {
        self.search_local()
    }
    pub(in crate::model) fn open_file_picker(&mut self) -> Vec<Effect> {
        self.open_convenience(Panel::Files, "")
    }
    pub(in crate::model) fn convenience_shortcut(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        let shortcut = self.chat.convenience.preferences.action(key)?;
        Some(self.run_shortcut(shortcut))
    }

    fn run_shortcut(&mut self, shortcut: Shortcut) -> Vec<Effect> {
        match shortcut {
            Shortcut::Queue => self.open_instruction_queue(),
            Shortcut::Find => self.open_convenience(Panel::Find, ""),
            Shortcut::Files => self.open_convenience(Panel::Files, ""),
            Shortcut::Keys => self.open_convenience(Panel::Keys, ""),
            Shortcut::Theme => self.open_convenience(Panel::Theme, ""),
            Shortcut::Copy => self.quick_copy(false),
            Shortcut::CopyCode => self.open_code_picker(),
            Shortcut::Editor => {
                self.start_local(LocalOperation::EditDraft { text: self.chat.buffer.clone() })
            }
            Shortcut::PasteImage => self.start_local(LocalOperation::ReadClipboardImage),
        }
    }

    pub(in crate::model::chat) fn convenience_command(
        &mut self,
        name: &str,
        arguments: &str,
    ) -> Vec<Effect> {
        match name {
            "find" => {
                self.clear_chat_command();
                self.open_convenience(Panel::Find, arguments)
            }
            "pick" => {
                self.clear_chat_command();
                self.open_convenience(Panel::Files, arguments)
            }
            "copy" => {
                self.clear_chat_command();
                if arguments == "code" { self.open_code_picker() } else { self.quick_copy(false) }
            }
            "editor" => {
                self.clear_chat_command();
                self.run_shortcut(Shortcut::Editor)
            }
            "paste-image" => {
                self.clear_chat_command();
                self.run_shortcut(Shortcut::PasteImage)
            }
            "theme" if !arguments.is_empty() => {
                if let Some(theme) = Theme::parse(arguments) {
                    self.apply_theme(theme);
                    self.clear_chat_command();
                } else {
                    self.notice(
                        NoticeLevel::Warning,
                        "Use /theme dark, light, or plain. Draft retained.",
                    );
                }
                Vec::new()
            }
            "theme" => {
                self.clear_chat_command();
                self.open_convenience(Panel::Theme, "")
            }
            "keys" => {
                if arguments == "reset" {
                    match self.chat.convenience.preferences.reset() {
                        Ok(()) => self.notice(
                            NoticeLevel::Info,
                            "Convenience shortcuts restored to defaults.",
                        ),
                        Err(e) => self.notice(NoticeLevel::Error, e),
                    }
                } else if !arguments.is_empty() {
                    let binding =
                        arguments.split_once(char::is_whitespace).and_then(|(name, key)| {
                            Shortcut::ALL
                                .into_iter()
                                .find(|a| a.name() == name)
                                .map(|a| (a, key.trim()))
                        });
                    match binding {
                        Some((a, k)) => self.set_shortcut(a, k),
                        None => self.notice(
                            NoticeLevel::Warning,
                            "Use /keys, /keys ACTION KEY, or /keys reset.",
                        ),
                    }
                }
                self.clear_chat_command();
                self.open_convenience(Panel::Keys, "")
            }
            _ => Vec::new(),
        }
    }

    fn open_convenience(&mut self, panel: Panel, query: &str) -> Vec<Effect> {
        self.chat.output_selection = None;
        let ui = &mut self.chat.convenience;
        ui.panel = Some(panel);
        ui.query = query.to_owned();
        ui.selected = 0;
        ui.scroll = 0;
        ui.preview = None;
        ui.capturing = false;
        ui.files.clear();
        ui.search = None;
        ui.error.clear();
        ui.result_query = None;
        if panel == Panel::Theme {
            ui.selected = match ui.preferences.theme {
                Theme::Dark => 0,
                Theme::Light => 1,
                Theme::Plain => 2,
            };
        }
        self.search_local()
    }

    fn start_local(&mut self, operation: LocalOperation) -> Vec<Effect> {
        if self.chat.convenience.pending.is_some() {
            return Vec::new();
        }
        let ui = &mut self.chat.convenience;
        ui.generation = ui.generation.wrapping_add(1);
        ui.loading = true;
        ui.pending = Some(PendingLocal {
            generation: ui.generation,
            query: ui.query.clone(),
            draft: self.chat.buffer.clone(),
            target: (self.chat.run_id, self.chat.workbench.selected),
        });
        vec![Effect::LocalOperation { generation: ui.generation, operation }]
    }

    fn search_local(&mut self) -> Vec<Effect> {
        let ui = &self.chat.convenience;
        let operation = match ui.panel {
            Some(Panel::Find) => {
                let Some(run) = self.chat.run_id else {
                    self.chat.convenience.error =
                        "Open a run to search its locally saved public transcript.".into();
                    return Vec::new();
                };
                if ui.query.is_empty() {
                    return Vec::new();
                }
                LocalOperation::SearchTranscript {
                    scope: crate::model::format_id(run.as_bytes()),
                    query: ui.query.clone(),
                }
            }
            Some(Panel::Files) => {
                let root = self
                    .product
                    .as_ref()
                    .map(|p| PathBuf::from(p.launch.workspace_label()))
                    .filter(|p| p.is_absolute() && p.is_dir())
                    .or_else(|| std::env::current_dir().ok());
                let Some(root) = root else {
                    return Vec::new();
                };
                let query = ui.query.clone();
                self.chat.convenience.file_root = Some(root.clone());
                LocalOperation::FindFiles { root, query }
            }
            _ => return Vec::new(),
        };
        self.start_local(operation)
    }

    pub(in crate::model) fn local_result(
        &mut self,
        generation: u64,
        result: Result<LocalReply, String>,
    ) -> Vec<Effect> {
        if !self.chat.convenience.pending.as_ref().is_some_and(|p| p.generation == generation) {
            return Vec::new();
        }
        let Some(pending) = self.chat.convenience.pending.take() else {
            return Vec::new();
        };
        self.chat.convenience.loading = false;
        if pending.target != (self.chat.run_id, self.chat.workbench.selected) {
            return Vec::new();
        }
        match result {
            Ok(LocalReply::Files(files)) => {
                if self.chat.convenience.panel != Some(Panel::Files) {
                    return Vec::new();
                }
                if pending.query != self.chat.convenience.query {
                    return self.search_local();
                }
                self.chat.convenience.files = files;
                self.chat.convenience.result_query = Some(pending.query.clone());
            }
            Ok(LocalReply::Transcript(page)) => {
                if self.chat.convenience.panel != Some(Panel::Find) {
                    return Vec::new();
                }
                if pending.query != self.chat.convenience.query {
                    return self.search_local();
                }
                self.chat.convenience.search = Some(page);
                self.chat.convenience.result_query = Some(pending.query.clone());
            }
            Ok(LocalReply::EditedDraft(text)) => {
                if self.chat.buffer != pending.draft {
                    self.notice(
                        NoticeLevel::Warning,
                        "Editor result arrived after the draft changed; newer draft retained.",
                    );
                    return Vec::new();
                }
                self.chat.buffer.clear();
                self.chat.cursor = 0;
                self.chat.selection_anchor = None;
                self.paste_chat_event(&text);
                self.notice(
                    NoticeLevel::Info,
                    "Editor draft restored. Review it and press Enter to send.",
                );
            }
            Ok(LocalReply::ClipboardImage(path)) => {
                if self.chat.buffer != pending.draft {
                    self.notice(
                        NoticeLevel::Warning,
                        "Clipboard result arrived after the draft changed. Paste again when ready.",
                    );
                    return Vec::new();
                }
                self.chat.workbench.images.preserve_composer = true;
                let effects = self.image_command(path.to_string_lossy().as_ref());
                self.chat.buffer = pending.draft;
                self.chat.cursor = self.chat.buffer.len();
                return effects;
            }
            Err(error) => {
                self.chat.convenience.error.clone_from(&error);
                self.notice(NoticeLevel::Warning, error);
            }
        }
        if pending.query != self.chat.convenience.query {
            self.chat.convenience.selected = 0;
            return self.search_local();
        }
        Vec::new()
    }

    pub(in crate::model) fn paste_convenience(&mut self, text: &str) -> bool {
        let ui = &mut self.chat.convenience;
        if !matches!(ui.panel, Some(Panel::Find | Panel::Files)) || ui.preview.is_some() {
            return ui.panel.is_some();
        }
        ui.query.extend(text.chars().filter(|c| !c.is_control()));
        ui.files.clear();
        ui.search = None;
        ui.result_query = None;
        ui.selected = 0;
        true
    }

    pub(in crate::model) fn convenience_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(panel) = self.chat.convenience.panel else {
            return Vec::new();
        };
        if key.code == KeyCode::Esc {
            let ui = &mut self.chat.convenience;
            if ui.preview.take().is_none() {
                ui.panel = None;
                ui.pending = None;
                ui.loading = false;
            }
            ui.scroll = 0;
            ui.capturing = false;
            return Vec::new();
        }
        if panel == Panel::Keys && self.chat.convenience.capturing {
            if let Some(name) = crate::preferences::key_name(key) {
                let action =
                    Shortcut::ALL[self.chat.convenience.selected.min(Shortcut::ALL.len() - 1)];
                self.set_shortcut(action, &name);
                self.chat.convenience.capturing = false;
            } else {
                self.notice(
                    NoticeLevel::Warning,
                    "Use Ctrl/Alt plus a letter, or an available function key. Esc cancels.",
                );
            }
            return Vec::new();
        }
        if self.chat.convenience.preview.is_some() {
            if key.code == KeyCode::Char('c') {
                if let Some(text) = self.chat.convenience.preview.clone() {
                    return self.copy_text(text);
                }
            }
            let scroll = &mut self.chat.convenience.scroll;
            match key.code {
                KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::Down => *scroll = scroll.saturating_add(1),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
                KeyCode::PageDown => *scroll = scroll.saturating_add(10),
                KeyCode::Home => *scroll = 0,
                _ => {}
            }
            return Vec::new();
        }
        let count = match panel {
            Panel::Find => self.chat.convenience.search.as_ref().map_or(0, |p| p.hits.len()),
            Panel::Files => self.chat.convenience.files.len(),
            Panel::Keys => Shortcut::ALL.len(),
            Panel::Theme => 3,
            Panel::Code => self.chat.convenience.blocks.len(),
        };
        if key.code == KeyCode::Enter
            && matches!(panel, Panel::Find | Panel::Files)
            && (self.chat.convenience.loading
                || self.chat.convenience.result_query.as_deref()
                    != Some(&self.chat.convenience.query))
        {
            return self.search_local();
        }
        match key.code {
            KeyCode::Up => {
                self.chat.convenience.selected = self.chat.convenience.selected.saturating_sub(1)
            }
            KeyCode::Down => {
                self.chat.convenience.selected =
                    (self.chat.convenience.selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::PageUp => {
                self.chat.convenience.selected = self.chat.convenience.selected.saturating_sub(10)
            }
            KeyCode::PageDown => {
                self.chat.convenience.selected =
                    (self.chat.convenience.selected + 10).min(count.saturating_sub(1))
            }
            KeyCode::Enter => match panel {
                Panel::Files => {
                    if let Some(path) =
                        self.chat.convenience.files.get(self.chat.convenience.selected).cloned()
                    {
                        let Some(path_text) = path.to_str() else {
                            self.notice(NoticeLevel::Warning,"This filename is not valid UTF-8 and cannot be attached through this interface.");
                            return Vec::new();
                        };
                        self.chat.convenience.panel = None;
                        self.chat.workbench.files.preserve_composer = true;
                        let effects = self.file_command(path_text);
                        return effects;
                    }
                }
                Panel::Find => {
                    self.chat.convenience.preview = self
                        .chat
                        .convenience
                        .search
                        .as_ref()
                        .and_then(|p| p.hits.get(self.chat.convenience.selected))
                        .map(|h| h.body.clone());
                    if self.chat.convenience.preview.is_none() {
                        return self.search_local();
                    }
                }
                Panel::Keys => self.chat.convenience.capturing = true,
                Panel::Theme => self.apply_theme(
                    [Theme::Dark, Theme::Light, Theme::Plain]
                        [self.chat.convenience.selected.min(2)],
                ),
                Panel::Code => {
                    if let Some((_, body)) =
                        self.chat.convenience.blocks.get(self.chat.convenience.selected).cloned()
                    {
                        return self.copy_text(body);
                    }
                }
            },
            KeyCode::Backspace if matches!(panel, Panel::Find | Panel::Files) => {
                self.chat.convenience.query.pop();
                self.invalidate_local_results();
                return self.search_local();
            }
            KeyCode::Tab if panel == Panel::Code => {
                self.chat.convenience.preview = self
                    .chat
                    .convenience
                    .blocks
                    .get(self.chat.convenience.selected)
                    .map(|(_, body)| body.clone());
                self.chat.convenience.scroll = 0;
            }
            KeyCode::Char(c)
                if matches!(panel, Panel::Find | Panel::Files)
                    && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.chat.convenience.query.push(c);
                self.invalidate_local_results();
                return self.search_local();
            }
            _ => {}
        }
        Vec::new()
    }

    fn invalidate_local_results(&mut self) {
        let ui = &mut self.chat.convenience;
        ui.selected = 0;
        ui.files.clear();
        ui.search = None;
        ui.result_query = None;
        ui.error.clear();
    }

    fn apply_theme(&mut self, theme: Theme) {
        let previous = self.chat.convenience.preferences.theme;
        self.chat.convenience.preferences.theme = theme;
        if let Err(error) = self.chat.convenience.preferences.save() {
            self.chat.convenience.preferences.theme = previous;
            self.notice(NoticeLevel::Error, error);
        } else {
            self.chat.formatted_messages.get_mut().clear();
            self.notice(NoticeLevel::Info, format!("Syntax theme: {}", theme.name()));
        }
    }

    fn set_shortcut(&mut self, action: Shortcut, key: &str) {
        match self.chat.convenience.preferences.set_binding(action, key) {
            Ok(()) => self.notice(NoticeLevel::Info, format!("{} uses {key}", action.name())),
            Err(e) => self.notice(NoticeLevel::Warning, e),
        }
    }

    fn quick_copy(&mut self, _code: bool) -> Vec<Effect> {
        let text = self.chat.snapshot.as_ref().and_then(|s| {
            let activities = s.activities();
            let end = activities
                .iter()
                .rposition(|a| a.kind() == peritus_app_protocol::ProductActivityKind::Assistant)?;
            let start = activities[..=end]
                .iter()
                .rposition(|a| a.kind() != peritus_app_protocol::ProductActivityKind::Assistant)
                .map_or(0, |i| i + 1);
            Some(
                activities[start..=end]
                    .iter()
                    .map(peritus_app_protocol::ProductActivity::text)
                    .collect::<String>(),
            )
        });
        match text {
            Some(t) => self.copy_text(t),
            None => {
                self.notice(NoticeLevel::Info, "No assistant response is available to copy.");
                Vec::new()
            }
        }
    }

    fn open_code_picker(&mut self) -> Vec<Effect> {
        let mut groups: Vec<String> = Vec::new();
        let mut continuing = false;
        if let Some(snapshot) = &self.chat.snapshot {
            for a in snapshot.activities() {
                if a.kind() == peritus_app_protocol::ProductActivityKind::Assistant {
                    if !continuing {
                        groups.push(String::new());
                    }
                    if let Some(group) = groups.last_mut() {
                        group.push_str(a.text());
                    }
                    continuing = true;
                } else {
                    continuing = false;
                }
            }
        }
        let mut blocks = Vec::new();
        for source in groups {
            let mut active: Option<(String, String)> = None;
            for (event, range) in pulldown_cmark::Parser::new(&source).into_offset_iter() {
                match event {
                    pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(kind)) => {
                        let language = match kind {
                            pulldown_cmark::CodeBlockKind::Fenced(language) => {
                                language.into_string()
                            }
                            pulldown_cmark::CodeBlockKind::Indented => String::new(),
                        };
                        active = Some((language, String::new()));
                    }
                    pulldown_cmark::Event::Text(text) => {
                        if let Some((_, body)) = &mut active {
                            let original = &source[range];
                            // Preserve original line endings when the Markdown token differs only
                            // by CRLF normalization; quoted/list prefixes remain parser-owned.
                            body.push_str(if original.replace("\r\n", "\n") == text.as_ref() {
                                original
                            } else {
                                &text
                            });
                        }
                    }
                    pulldown_cmark::Event::End(pulldown_cmark::TagEnd::CodeBlock) => {
                        if let Some(block) = active.take() {
                            blocks.push(block);
                        }
                    }
                    _ => {}
                }
            }
        }
        if blocks.is_empty() {
            self.notice(NoticeLevel::Info, "No complete fenced code blocks are available to copy.");
            return Vec::new();
        }
        blocks.reverse();
        self.chat.convenience.blocks = blocks;
        self.chat.convenience.panel = Some(Panel::Code);
        self.chat.convenience.selected = 0;
        Vec::new()
    }

    fn copy_text(&mut self, text: String) -> Vec<Effect> {
        let Ok(operation) =
            peritus_app_protocol::ControlOperationId::new(self.ids.bytes(b"quick-copy"))
        else {
            return Vec::new();
        };
        vec![Effect::CopyText { operation, text }]
    }
}
