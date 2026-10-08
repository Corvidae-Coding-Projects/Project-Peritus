//! Bounded local editing for pipe-backed children that have no terminal line discipline.

use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug)]
pub(super) struct LineInput {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) pending: bool,
    last_output_cr: bool,
    maximum: usize,
}

impl LineInput {
    pub(super) const fn new(maximum: usize) -> Self {
        Self { text: String::new(), cursor: 0, pending: false, last_output_cr: false, maximum }
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> Option<Vec<u8>> {
        if self.pending || !crate::input::is_active_key(key) {
            return None;
        }
        if key.code == KeyCode::Enter {
            let mut bytes = self.text.as_bytes().to_vec();
            bytes.push(b'\n');
            self.pending = true;
            return Some(bytes);
        }
        if matches!(key.code, KeyCode::Char(value) if value.is_control()) {
            return None;
        }
        let mut text = self.text.clone();
        let mut cursor = self.cursor;
        crate::input::edit_text(&mut text, &mut cursor, key);
        if text.len() < self.maximum {
            self.text = text;
            self.cursor = cursor;
        }
        None
    }

    pub(super) fn paste(&mut self, text: &str) {
        if self.pending {
            return;
        }
        let text = text.replace("\r\n", "\n");
        let text = text
            .chars()
            .filter(|value| !value.is_control() || matches!(value, '\n' | '\t'))
            .collect::<String>();
        if self.text.len().saturating_add(text.len()) < self.maximum {
            self.text.insert_str(self.cursor, &text);
            self.cursor += text.len();
        }
    }

    pub(super) fn settle(&mut self, accepted: bool) {
        if accepted && self.pending {
            self.text.clear();
            self.cursor = 0;
        }
        self.pending = false;
    }

    pub(super) const fn discontinuity(&mut self) {
        self.last_output_cr = false;
    }

    pub(super) fn display_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut display = Vec::with_capacity(bytes.len());
        for byte in bytes {
            if *byte == b'\n' && !self.last_output_cr {
                display.push(b'\r');
            }
            display.push(*byte);
            self.last_output_cr = *byte == b'\r';
        }
        display
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn unicode_input_reserves_a_newline_and_paste_never_submits_controls() {
        let mut line = LineInput::new(5);
        line.paste("λ\r\n\x03x");
        assert_eq!(line.text, "λ\nx");
        line.paste("y");
        assert_eq!(line.text, "λ\nx", "oversized paste must preserve the existing draft");
        assert_eq!(
            line.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap(),
            "λ\nx\n".as_bytes()
        );
        line.paste("z");
        assert_eq!(line.text, "λ\nx", "pending input cannot change before acknowledgement");
    }

    #[test]
    fn pipe_newlines_start_at_column_zero_across_chunk_boundaries() {
        let mut line = LineInput::new(8192);
        assert_eq!(line.display_bytes(b"out\nerr\r"), b"out\r\nerr\r");
        assert_eq!(line.display_bytes(b"\nnext\n"), b"\nnext\r\n");
    }
}
