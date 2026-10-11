//! Logical terminal history shared by attachments and complete preview records.

use std::cell::RefCell;

pub(super) struct Transcript {
    parser: RefCell<vt100::Parser>,
    boundary: crate::sanitize::TerminalSanitizer,
}

impl core::fmt::Debug for Transcript {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Transcript")
            .field("size", &self.parser.borrow().screen().size())
            .finish_non_exhaustive()
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self {
            parser: RefCell::new(vt100::Parser::new(24, 80, usize::MAX)),
            boundary: crate::sanitize::TerminalSanitizer::default(),
        }
    }
}

impl Transcript {
    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.boundary.push(bytes);
        self.parser.get_mut().process(bytes);
    }

    pub(super) fn finish(&mut self) {
        let tokens = self.boundary.finish();
        if tokens.is_empty() {
            return;
        }
        // CAN and ST are trusted parser input, never emitted to the host terminal.
        // They terminate an incomplete control string before its inert diagnostic is rendered.
        self.parser.get_mut().process(b"\x18\x1b\\");
        let diagnostic = tokens
            .into_iter()
            .filter_map(|token| match token {
                crate::sanitize::SafeToken::Character(value) => Some(value),
                _ => None,
            })
            .collect::<String>();
        self.parser.get_mut().process(diagnostic.as_bytes());
    }

    pub(super) fn resize(&mut self, rows: u16, columns: u16) {
        self.parser.get_mut().screen_mut().set_size(rows.clamp(1, 256), columns.clamp(1, 4096));
    }

    pub(super) fn display_lines(&self) -> Vec<String> {
        let mut parser = self.parser.borrow_mut();
        let screen = parser.screen_mut();
        let prior_offset = screen.scrollback();
        let columns = screen.size().1;
        screen.set_scrollback(usize::MAX);
        let retained = screen.scrollback();
        let mut lines = Vec::with_capacity(retained + usize::from(screen.size().0));
        for offset in (1..=retained).rev() {
            screen.set_scrollback(offset);
            lines.extend(screen.rows(0, columns).take(1));
        }
        screen.set_scrollback(0);
        lines.extend(screen.contents().split('\n').map(str::to_owned));
        screen.set_scrollback(prior_offset);
        lines
    }

    pub(super) fn visible_lines(&self, height: usize, scroll: usize) -> Vec<String> {
        let mut parser = self.parser.borrow_mut();
        let screen = parser.screen_mut();
        let prior_offset = screen.scrollback();
        screen.set_scrollback(scroll);
        let lines = screen.rows(0, screen.size().1).take(height).collect();
        screen.set_scrollback(prior_offset);
        lines
    }

    pub(super) fn cursor(&self) -> Option<(u16, u16)> {
        (!self.parser.borrow().screen().hide_cursor())
            .then(|| self.parser.borrow().screen().cursor_position())
    }

    pub(super) fn application_cursor(&self) -> bool {
        self.parser.borrow().screen().application_cursor()
    }

    pub(super) fn bracketed_paste(&self) -> bool {
        self.parser.borrow().screen().bracketed_paste()
    }
}
