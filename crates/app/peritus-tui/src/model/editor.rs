//! Modal editor keyboard handling and typed submission routing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{AppModel, Editor, EditorKind};
use crate::{action::Effect, input::edit_text};

impl AppModel {
    pub(super) fn handle_editor_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.code == KeyCode::Esc {
            self.editor = None;
            return Vec::new();
        }
        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
            if let Some(editor) = &mut self.editor
                && matches!(editor.kind, EditorKind::ProductTask | EditorKind::ProductMessage(_))
            {
                editor.buffer.insert(editor.cursor, '\n');
                editor.cursor += 1;
            }
            return Vec::new();
        }
        if key.code == KeyCode::Enter {
            let Some(editor) = self.editor.take() else {
                return Vec::new();
            };
            return self.submit_editor(editor);
        }
        if let Some(editor) = &mut self.editor {
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
        }
    }
}
