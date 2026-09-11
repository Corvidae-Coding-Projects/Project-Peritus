//! Keyboard actions are separate from inert draft editing and explicit confirmation.
use super::{AppModel, Effect, WorkbenchIntent};
use crossterm::event::{KeyCode, KeyEvent};

impl AppModel {
    pub(in crate::model::chat::workbench) fn file_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<Vec<Effect>> {
        if let Some(field) = self.chat.workbench.files.editing {
            let file = &mut self.chat.workbench.files;
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                file.editing = None;
                return Some(Vec::new());
            }
            let (text, limit) = match field {
                0 => (&mut file.path, 4096),
                1 => (&mut file.range, 80),
                _ => (&mut file.caption, 8192),
            };
            if let KeyCode::Char(character) = key.code
                && (character.is_control() || text.len() + character.len_utf8() > limit)
            {
                return Some(Vec::new());
            }
            crate::input::edit_text(text, &mut file.cursor, key);
            file.discard_preview();
            return Some(Vec::new());
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            return None;
        }
        if key.code == KeyCode::Char('r') {
            self.chat.workbench.snapshot = None;
            self.chat.workbench.files.discard_preview();
            self.chat.workbench.files.page = None;
            return Some(self.refresh_file_panel());
        }
        if self.chat.workbench.files.list && !matches!(key.code, KeyCode::Char('i' | 'g' | 't')) {
            return self.file_list_key(key);
        }
        match key.code {
            KeyCode::Char('i' | 'g' | 't') => {
                let file = &mut self.chat.workbench.files;
                file.list = false;
                let (field, count) = match key.code {
                    KeyCode::Char('i') => (0, file.path.len()),
                    KeyCode::Char('g') => (1, file.range.len()),
                    _ => (2, file.caption.len()),
                };
                file.editing = Some(field);
                file.cursor = count;
            }
            KeyCode::Char('m') => {
                self.chat.workbench.files.refresh = !self.chat.workbench.files.refresh;
                self.chat.workbench.files.discard_preview();
            }
            KeyCode::Char('p') => return Some(self.preview_file()),
            KeyCode::Char('c') => return Some(self.confirm_file()),
            KeyCode::Char('l') => {
                self.chat.workbench.files.list = true;
                return Some(self.refresh_file_panel());
            }
            KeyCode::Char('x') => self.chat.workbench.files.discard_preview(),
            _ => return None,
        }
        Some(Vec::new())
    }
    pub(in crate::model::chat) fn paste_file_field(&mut self, text: &str) -> bool {
        if !self.chat.workbench.open || !self.chat.workbench.files.open {
            return false;
        }
        let file = &mut self.chat.workbench.files;
        let Some(field) = file.editing else { return true };
        let (buffer, limit) = match field {
            0 => (&mut file.path, 4096),
            1 => (&mut file.range, 80),
            _ => (&mut file.caption, 8192),
        };
        if text.chars().any(char::is_control) || buffer.len().saturating_add(text.len()) > limit {
            self.notice(
                super::NoticeLevel::Warning,
                "Pasted field is oversized or contains controls; nothing changed.",
            );
            return true;
        }
        buffer.insert_str(file.cursor, text);
        file.cursor += text.len();
        file.discard_preview();
        true
    }
    fn file_list_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        let file = &mut self.chat.workbench.files;
        let page = file.page.as_ref()?;
        match key.code {
            KeyCode::Left => file.selected = file.selected.saturating_sub(1),
            KeyCode::Right => {
                file.selected = (file.selected + 1).min(page.rows().len().saturating_sub(1));
            }
            KeyCode::Char(' ') => {
                let row = page.rows().get(file.selected)?;
                let intent = WorkbenchIntent::SelectFile {
                    attachment: row.attachment(),
                    selected: !row.selected(),
                };
                let query = page.query();
                return Some(self.send_file_intent(query.query(), query.revision(), intent));
            }
            KeyCode::Char('n' | 'b') => {
                let offset = if key.code == KeyCode::Char('n') {
                    page.query().offset().saturating_add(32)
                } else {
                    page.query().offset().saturating_sub(32)
                };
                if offset >= page.total() {
                    return Some(Vec::new());
                }
                let revision = page.query().revision();
                return Some(self.file_page(revision, offset));
            }
            _ => return None,
        }
        self.chat.workbench.scroll = 0;
        Some(Vec::new())
    }
}
