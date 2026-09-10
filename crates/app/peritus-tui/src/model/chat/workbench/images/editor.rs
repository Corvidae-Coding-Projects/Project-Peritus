//! Local path/caption editing is inert; only separate keyboard actions read or confirm.

use super::{AppModel, Effect, ImageField, NoticeLevel};
use crossterm::event::{KeyCode, KeyEvent};

impl AppModel {
    pub(in crate::model::chat::workbench) fn image_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<Vec<Effect>> {
        if self.chat.workbench.images.editing.is_some() {
            self.edit_image_field(key);
            return Some(Vec::new());
        }
        if self.workbench_request_pending() || self.chat.workbench.unresolved.is_some() {
            return None;
        }
        if self.chat.workbench.images.list {
            return self.image_page_key(key);
        }
        match key.code {
            KeyCode::Char('l') => {
                self.chat.workbench.images.list = true;
                return Some(self.refresh_image_page(0, 0));
            }
            KeyCode::Char('i') => self.focus_image_field(ImageField::Path),
            KeyCode::Char('t') => self.focus_image_field(ImageField::Caption),
            KeyCode::Char('p') => return Some(self.begin_image_read()),
            KeyCode::Char('c') => return Some(self.confirm_image()),
            KeyCode::Char('x') => {
                self.chat.workbench.images.discard_preview();
                "Preview discarded. No image was included or sent to a provider; local upload history may remain.".clone_into(&mut self.chat.workbench.message);
            }
            _ => return None,
        }
        Some(Vec::new())
    }
    const fn focus_image_field(&mut self, field: ImageField) {
        let image = &mut self.chat.workbench.images;
        image.editing = Some(field);
        image.cursor = match field {
            ImageField::Path => image.path.len(),
            ImageField::Caption => image.caption.len(),
        };
        self.chat.workbench.scroll = 0;
    }
    fn edit_image_field(&mut self, key: KeyEvent) {
        let image = &mut self.chat.workbench.images;
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
            image.editing = None;
            return;
        }
        let Some(field) = image.editing else {
            return;
        };
        let (buffer, limit) = match field {
            ImageField::Path => (&mut image.path, 4096),
            ImageField::Caption => (&mut image.caption, 8192),
        };
        if let KeyCode::Char(character) = key.code
            && (character.is_control() || buffer.len().saturating_add(character.len_utf8()) > limit)
        {
            return;
        }
        let before = buffer.clone();
        let _ = crate::input::edit_text(buffer, &mut image.cursor, key);
        if field == ImageField::Path && before != *buffer {
            image.preview = None;
        }
    }
    pub(in crate::model::chat) fn paste_image_field(&mut self, text: &str) -> bool {
        if !self.chat.workbench.open || !self.chat.workbench.images.open {
            return false;
        }
        let image = &mut self.chat.workbench.images;
        let Some(field) = image.editing else {
            return true;
        };
        let (buffer, limit) = match field {
            ImageField::Path => (&mut image.path, 4096),
            ImageField::Caption => (&mut image.caption, 8192),
        };
        if text.chars().any(char::is_control) || buffer.len().saturating_add(text.len()) > limit {
            self.notice(
                NoticeLevel::Warning,
                "Pasted field is oversized or contains controls; nothing changed.",
            );
            return true;
        }
        buffer.insert_str(image.cursor, text);
        image.cursor += text.len();
        if field == ImageField::Path {
            image.preview = None;
        }
        true
    }
}
