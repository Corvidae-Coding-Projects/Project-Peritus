//! Bounded insertion shared by keyboard text, newlines, and bracketed paste.

use crate::model::{AppModel, EditorKind, NoticeLevel};

impl AppModel {
    pub(in crate::model) fn paste_editor_event(&mut self, text: &str) {
        self.paste_editor(text);
        if let Some(editor) = &mut self.editor
            && matches!(editor.kind, EditorKind::ProductMessage(_))
            && editor.buffer.trim_start().starts_with('/')
        {
            editor.pasted_command = true;
        }
    }

    pub(in crate::model) fn paste_editor(&mut self, text: &str) {
        let Some(editor) = &mut self.editor else { return };
        let maximum = match editor.kind {
            EditorKind::ProductMessage(_) | EditorKind::ReviewFeedback(_) => {
                peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES
            }
            // Leave room for pasted whitespace and a fixable typo around a 32-digit identifier.
            EditorKind::ProcessId => 256,
            EditorKind::ApprovalSignature(_) => {
                self.limits.codec().max_frame_bytes.div_ceil(3).saturating_mul(4)
            }
            EditorKind::PromptAnswer(_) => self.limits.codec().max_string_bytes,
        };
        let text = text
            .chars()
            .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
            .take(maximum.saturating_add(1))
            .collect::<String>();
        if editor.buffer.len().saturating_add(text.len()) > maximum {
            self.notice(NoticeLevel::Warning, format!("Input exceeds {maximum} bytes; existing draft retained. Paste a smaller selection."));
            return;
        }
        editor.buffer.insert_str(editor.cursor, &text);
        editor.cursor += text.len();
    }
}
