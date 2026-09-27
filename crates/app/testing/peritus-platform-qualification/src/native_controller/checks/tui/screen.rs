//! Small bounded VT screen projection for assertions against differential TUI frames.

#[derive(Clone, Copy, Debug, Default)]
enum ParserState {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

pub(super) struct TerminalScreen {
    rows: usize,
    columns: usize,
    cells: Vec<char>,
    row: usize,
    column: usize,
    saved: (usize, usize),
    parser: ParserState,
    parameters: Vec<u8>,
    utf8_continuations: u8,
}

impl TerminalScreen {
    pub(super) fn new(rows: usize, columns: usize) -> Self {
        Self {
            rows,
            columns,
            cells: vec![' '; rows.saturating_mul(columns)],
            row: 0,
            column: 0,
            saved: (0, 0),
            parser: ParserState::Ground,
            parameters: Vec::with_capacity(24),
            utf8_continuations: 0,
        }
    }

    pub(super) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.feed_byte(byte);
        }
    }

    pub(super) fn contains(&self, needle: &str) -> bool {
        self.cells.chunks(self.columns).any(|row| row.iter().collect::<String>().contains(needle))
    }

    pub(super) fn diagnostic(&self) -> String {
        self.cells
            .chunks(self.columns)
            .map(|row| row.iter().collect::<String>().trim_end().to_owned())
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn feed_byte(&mut self, byte: u8) {
        match self.parser {
            ParserState::Ground => self.feed_ground(byte),
            ParserState::Escape => match byte {
                b'[' => {
                    self.parameters.clear();
                    self.parser = ParserState::Csi;
                }
                b']' => self.parser = ParserState::Osc,
                b'7' => {
                    self.saved = (self.row, self.column);
                    self.parser = ParserState::Ground;
                }
                b'8' => {
                    (self.row, self.column) = self.saved;
                    self.parser = ParserState::Ground;
                }
                _ => self.parser = ParserState::Ground,
            },
            ParserState::Csi if (0x40..=0x7e).contains(&byte) => {
                self.apply_csi(byte);
                self.parser = ParserState::Ground;
            }
            ParserState::Csi => {
                if self.parameters.len() < 32 {
                    self.parameters.push(byte);
                }
            }
            ParserState::Osc if byte == 0x07 => self.parser = ParserState::Ground,
            ParserState::Osc if byte == 0x1b => self.parser = ParserState::OscEscape,
            ParserState::Osc => {}
            ParserState::OscEscape if byte == b'\\' => self.parser = ParserState::Ground,
            ParserState::OscEscape => self.parser = ParserState::Osc,
        }
    }

    fn feed_ground(&mut self, byte: u8) {
        if self.utf8_continuations > 0 {
            self.utf8_continuations -= 1;
            return;
        }
        match byte {
            0x1b => self.parser = ParserState::Escape,
            b'\r' => self.column = 0,
            b'\n' => self.row = (self.row + 1).min(self.rows.saturating_sub(1)),
            b'\x08' => self.column = self.column.saturating_sub(1),
            b'\t' => self.column = ((self.column / 8 + 1) * 8).min(self.columns),
            0x20..=0x7e => self.write(char::from(byte)),
            0xc0..=0xdf => self.start_utf8(1),
            0xe0..=0xef => self.start_utf8(2),
            0xf0..=0xf7 => self.start_utf8(3),
            _ => {}
        }
    }

    fn start_utf8(&mut self, continuations: u8) {
        self.write(' ');
        self.utf8_continuations = continuations;
    }

    fn write(&mut self, character: char) {
        if self.row < self.rows && self.column < self.columns {
            self.cells[self.row * self.columns + self.column] = character;
        }
        self.column = (self.column + 1).min(self.columns);
    }

    fn apply_csi(&mut self, command: u8) {
        let parameters = String::from_utf8_lossy(&self.parameters);
        let values = parameters
            .trim_start_matches(['?', '>', '<'])
            .split(';')
            .map(|value| value.parse::<usize>().ok())
            .collect::<Vec<_>>();
        let value = |index: usize, default: usize| {
            values
                .get(index)
                .and_then(|value| *value)
                .filter(|value| *value != 0)
                .unwrap_or(default)
        };
        match command {
            b'H' | b'f' => {
                self.row = value(0, 1).saturating_sub(1).min(self.rows.saturating_sub(1));
                self.column = value(1, 1).saturating_sub(1).min(self.columns.saturating_sub(1));
            }
            b'A' => self.row = self.row.saturating_sub(value(0, 1)),
            b'B' => self.row = (self.row + value(0, 1)).min(self.rows.saturating_sub(1)),
            b'C' => self.column = (self.column + value(0, 1)).min(self.columns),
            b'D' => self.column = self.column.saturating_sub(value(0, 1)),
            b'G' => self.column = value(0, 1).saturating_sub(1).min(self.columns),
            b'd' => self.row = value(0, 1).saturating_sub(1).min(self.rows.saturating_sub(1)),
            b'J' if values.first().and_then(|value| *value) == Some(2) => self.cells.fill(' '),
            b'K' => self.erase_line(values.first().and_then(|value| *value).unwrap_or(0)),
            b'X' => self.erase_characters(value(0, 1)),
            b's' => self.saved = (self.row, self.column),
            b'u' => (self.row, self.column) = self.saved,
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: usize) {
        let start = self.row.saturating_mul(self.columns);
        let (from, to) = match mode {
            1 => (start, start + self.column.saturating_add(1).min(self.columns)),
            2 => (start, start + self.columns),
            _ => (start + self.column.min(self.columns), start + self.columns),
        };
        self.cells[from..to].fill(' ');
    }

    fn erase_characters(&mut self, count: usize) {
        let start = self.row.saturating_mul(self.columns) + self.column.min(self.columns);
        let end = start.saturating_add(count).min((self.row + 1).saturating_mul(self.columns));
        self.cells[start..end].fill(' ');
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalScreen;

    #[test]
    fn differential_frames_reconstruct_the_visible_help_title() {
        let mut screen = TerminalScreen::new(8, 40);
        screen.feed(b"\x1b[2J\x1b[1;1HPeritus\x1b[4;3HRuns events");
        for chunk in [b"\x1b[4;3HKey r".as_slice(), b"\x1b[4;9Hf", b"\x1b[4;11Hrence"] {
            screen.feed(chunk);
        }

        assert!(screen.contains("Peritus"));
        assert!(screen.contains("Key reference"));
    }
}
