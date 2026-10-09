//! Bounded terminal emulation shared by attachments and preview output.

const MAX_SCROLLBACK_LINES: usize = 2048;

pub(super) struct Transcript {
    parser: vt100::Parser,
}

impl core::fmt::Debug for Transcript {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Transcript")
            .field("size", &self.parser.screen().size())
            .finish_non_exhaustive()
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self { parser: vt100::Parser::new(24, 80, MAX_SCROLLBACK_LINES) }
    }
}

impl Transcript {
    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub(super) fn resize(&mut self, rows: u16, columns: u16) {
        self.parser.screen_mut().set_size(rows.clamp(1, 256), columns.clamp(1, 4096));
    }

    pub(super) fn display_lines(&self) -> Vec<String> {
        let mut screen = self.parser.screen().clone();
        let columns = screen.size().1;
        screen.set_scrollback(MAX_SCROLLBACK_LINES);
        let retained = screen.scrollback();
        let mut lines = Vec::with_capacity(retained + usize::from(screen.size().0));
        for offset in (1..=retained).rev() {
            screen.set_scrollback(offset);
            lines.extend(screen.rows(0, columns).take(1));
        }
        screen.set_scrollback(0);
        lines.extend(screen.contents().split('\n').map(str::to_owned));
        lines
    }

    pub(super) fn visible_lines(&self, height: usize, scroll: usize) -> Vec<String> {
        let mut screen = self.parser.screen().clone();
        screen.set_scrollback(scroll);
        screen.rows(0, screen.size().1).take(height).collect()
    }

    pub(super) fn cursor(&self) -> Option<(u16, u16)> {
        (!self.parser.screen().hide_cursor()).then(|| self.parser.screen().cursor_position())
    }

    pub(super) fn application_cursor(&self) -> bool {
        self.parser.screen().application_cursor()
    }

    pub(super) fn bracketed_paste(&self) -> bool {
        self.parser.screen().bracketed_paste()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn actual_readline_cursor_sequences_and_alternate_screen_are_rendered() {
        let mut transcript = super::Transcript::default();
        transcript.push(
            b">>> \x1b[4D\x1b[4C\x1b[4D>>> p\x1b[5D\x1b[5C\x1b[5D>>> print(9*9)\r\n81\r\n>>> ",
        );
        let before = transcript.display_lines();
        assert_eq!(before, [">>> print(9*9)", "81", ">>> "]);
        transcript.push(b"\x1b[?1049h\x1b[2J\x1b[Htemporary screen");
        assert_eq!(transcript.display_lines(), ["temporary screen"]);
        transcript.push(b"\x1b[?1049l");
        assert_eq!(transcript.display_lines(), before);
    }

    #[test]
    fn readline_redraw_and_progress_carriage_returns_replace_the_current_line() {
        let lines = super::super::preview_lines(
            ">>> \r>>> p\r>>> pr\r>>> print(7*8)\r\n56\r\n\u{1b}[32m>>> \u{1b}[0m",
        );
        assert_eq!(lines, [">>> print(7*8)", "56", ">>> "]);
    }
}
