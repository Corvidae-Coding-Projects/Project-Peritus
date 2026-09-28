//! Modal editor keyboard handling and typed submission routing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{AppModel, Editor, EditorKind};
use crate::{action::Effect, input::edit_text};

mod input;

impl AppModel {
    pub(super) fn handle_editor_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && self
                .editor
                .as_ref()
                .is_some_and(|editor| matches!(editor.kind, EditorKind::ReviewFeedback(_)))
        {
            if key.code == KeyCode::Char('f') {
                return self.refresh_review();
            }
            if key.code == KeyCode::Char('b') {
                self.rebind_review_draft();
                return Vec::new();
            }
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            return self.request_reconnect();
        }
        if key.code == KeyCode::Esc {
            self.editor = None;
            return Vec::new();
        }
        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
            if let Some(editor) = &self.editor
                && matches!(
                    editor.kind,
                    EditorKind::ProductTask
                        | EditorKind::ProductMessage(_)
                        | EditorKind::ReviewFeedback(_)
                )
            {
                self.paste_editor("\n");
            }
            return Vec::new();
        }
        if key.code == KeyCode::Enter {
            let Some(editor) = self.editor.take() else {
                return Vec::new();
            };
            let retained = editor.clone();
            let existing = self.pending.keys().copied().collect::<std::collections::HashSet<_>>();
            let effects = self.submit_editor(editor);
            if effects.is_empty() {
                // Some specialized editors already reopen themselves on validation failure.
                if self.editor.is_none() {
                    self.restore_editor(retained, false);
                }
                if self.context.is_none() {
                    self.notice(super::NoticeLevel::Warning, "Disconnected; draft retained. Ctrl-R reconnects without closing this editor.");
                }
            } else if !matches!(retained.kind, EditorKind::ReviewFeedback(_)) {
                // Review comments retain their text and target in the durable-control
                // receipt state. Reopen them only after rejection is confirmed.
                for request in self.pending.keys().filter(|request| !existing.contains(request)) {
                    self.pending_editor_drafts.insert(*request, retained.clone());
                }
            }
            return effects;
        }
        if let KeyCode::Char(character) = key.code
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.paste_editor(&character.to_string());
            return Vec::new();
        }
        if let Some(editor) = &mut self.editor {
            if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                let width = self.chat.viewport.map_or(80, |area| {
                    crate::input::composer::modal_area(area).width.saturating_sub(2)
                });
                editor.cursor = crate::input::composer::vertical_cursor(
                    &editor.buffer,
                    editor.cursor,
                    key.code == KeyCode::Down,
                    usize::from(width),
                );
                return Vec::new();
            }
            let _ = edit_text(&mut editor.buffer, &mut editor.cursor, key);
        }
        Vec::new()
    }

    fn submit_editor(&mut self, editor: Editor) -> Vec<Effect> {
        match editor.kind {
            EditorKind::ProcessId => self.attach_terminal(&editor.buffer),
            EditorKind::ApprovalSignature(prompt_id) => {
                self.submit_signed_approval(prompt_id, &editor.buffer)
            }
            EditorKind::PromptAnswer(prompt_id) => self.submit_user_input(prompt_id, editor.buffer),
            EditorKind::ProductTask => self.submit_product_task(editor.buffer),
            EditorKind::ProductMessage(run_id) => {
                self.submit_product_message(run_id, editor.buffer)
            }
            EditorKind::ReviewFeedback(target) => {
                self.submit_review_feedback(*target, editor.buffer)
            }
        }
    }

    pub(super) fn open_editor(&mut self, editor: Editor) {
        let retained = self
            .retained_editors
            .iter()
            .position(|draft| match (&draft.kind, &editor.kind) {
                (EditorKind::ReviewFeedback(saved), EditorKind::ReviewFeedback(current)) => {
                    saved.query == current.query && saved.feedback == current.feedback
                }
                _ => draft.kind == editor.kind,
            })
            .and_then(|index| self.retained_editors.remove(index));
        self.editor = Some(retained.unwrap_or(editor));
    }

    pub(in crate::model) fn restore_editor(&mut self, mut editor: Editor, ambiguous: bool) {
        if ambiguous {
            editor.hint = "Connection interrupted. Inspect the current state before resending; this request may already have been accepted.";
        }
        if self.editor.is_none() {
            self.editor = Some(editor);
        } else {
            self.retained_editors.push_back(editor);
        }
    }

    pub(super) fn recover_editor_drafts(&mut self) {
        for editor in std::mem::take(&mut self.pending_editor_drafts).into_values() {
            self.restore_editor(editor, true);
        }
    }
}
