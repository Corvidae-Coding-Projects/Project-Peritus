//! Streaming terminal-output sanitization.

/// Removes active terminal controls from multiline conversational text.
pub fn sanitize_display_text(text: &str) -> String {
    let mut sanitizer = TerminalSanitizer::default();
    let mut tokens = sanitizer.push(text.as_bytes());
    tokens.extend(sanitizer.finish());
    tokens
        .into_iter()
        .filter_map(|token| match token {
            SafeToken::Character(character) if !character.is_control() => Some(character),
            SafeToken::Newline => Some('\n'),
            SafeToken::Tab => Some(' '),
            _ => None,
        })
        .collect()
}

/// A display-safe terminal editing token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeToken {
    Character(char),
    Newline,
    CarriageReturn,
    Backspace,
    Tab,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum EscapeState {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    String,
    StringEscape,
}

/// Stateful sanitizer that removes terminal control sequences across chunk boundaries.
#[derive(Debug, Default)]
pub struct TerminalSanitizer {
    state: EscapeState,
    utf8_tail: Vec<u8>,
}

impl TerminalSanitizer {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<SafeToken> {
        let mut visible = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            match self.state {
                EscapeState::Ground => match byte {
                    0x1b => {
                        self.finish_incomplete_utf8(&mut visible);
                        self.state = EscapeState::Escape;
                    }
                    b'\n' => {
                        self.finish_incomplete_utf8(&mut visible);
                        visible.push(SafeToken::Newline);
                    }
                    b'\r' => {
                        self.finish_incomplete_utf8(&mut visible);
                        visible.push(SafeToken::CarriageReturn);
                    }
                    0x08 | 0x7f => {
                        self.finish_incomplete_utf8(&mut visible);
                        visible.push(SafeToken::Backspace);
                    }
                    b'\t' => {
                        self.finish_incomplete_utf8(&mut visible);
                        visible.push(SafeToken::Tab);
                    }
                    0x20..=0x7e | 0x80..=0xff => self.push_utf8(byte, &mut visible),
                    _ => {}
                },
                EscapeState::Escape => {
                    self.state = match byte {
                        b'[' => EscapeState::Csi,
                        b']' => EscapeState::Osc,
                        b'P' | b'X' | b'^' | b'_' => EscapeState::String,
                        _ => EscapeState::Ground,
                    };
                }
                EscapeState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.state = EscapeState::Ground;
                    }
                }
                EscapeState::Osc => match byte {
                    0x07 => self.state = EscapeState::Ground,
                    0x1b => self.state = EscapeState::OscEscape,
                    _ => {}
                },
                EscapeState::OscEscape => {
                    self.state = if byte == b'\\' { EscapeState::Ground } else { EscapeState::Osc };
                }
                EscapeState::String => {
                    if byte == 0x1b {
                        self.state = EscapeState::StringEscape;
                    }
                }
                EscapeState::StringEscape => {
                    self.state =
                        if byte == b'\\' { EscapeState::Ground } else { EscapeState::String };
                }
            }
        }
        visible
    }

    /// Ends a complete stream or record, exposes incomplete control state, and resets the parser.
    /// Ordinary chunks must use `push` without finishing so split controls stay inert.
    pub(crate) fn finish(&mut self) -> Vec<SafeToken> {
        let mut visible = Vec::new();
        self.finish_incomplete_utf8(&mut visible);
        if self.state != EscapeState::Ground {
            visible.extend("[incomplete terminal control]".chars().map(SafeToken::Character));
        }
        self.state = EscapeState::Ground;
        visible
    }

    fn push_utf8(&mut self, byte: u8, visible: &mut Vec<SafeToken>) {
        if self.utf8_tail.is_empty() && byte.is_ascii() {
            visible.push(SafeToken::Character(char::from(byte)));
            return;
        }
        self.utf8_tail.push(byte);
        loop {
            match std::str::from_utf8(&self.utf8_tail) {
                Ok(text) => {
                    visible.extend(text.chars().map(SafeToken::Character));
                    self.utf8_tail.clear();
                    return;
                }
                Err(error) if error.valid_up_to() > 0 => {
                    let valid = error.valid_up_to();
                    let text = std::str::from_utf8(&self.utf8_tail[..valid])
                        .expect("valid_up_to identifies valid UTF-8");
                    visible.extend(text.chars().map(SafeToken::Character));
                    self.utf8_tail.drain(..valid);
                }
                Err(error) => {
                    let Some(invalid) = error.error_len() else {
                        return;
                    };
                    visible.push(SafeToken::Character('\u{fffd}'));
                    self.utf8_tail.drain(..invalid);
                }
            }
        }
    }

    fn finish_incomplete_utf8(&mut self, visible: &mut Vec<SafeToken>) {
        if !self.utf8_tail.is_empty() {
            visible.push(SafeToken::Character('\u{fffd}'));
            self.utf8_tail.clear();
        }
    }
}

/// Converts arbitrary bytes to bounded, non-control display text.
pub fn inert_preview(bytes: &[u8], maximum: usize) -> String {
    let mut output = String::with_capacity(maximum.min(bytes.len()));
    for &byte in bytes.iter().take(maximum) {
        match byte {
            b'\n' => output.push('↵'),
            b'\r' => output.push('↩'),
            b'\t' => output.push('⇥'),
            0x20..=0x7e => output.push(char::from(byte)),
            _ => {
                use core::fmt::Write as _;
                let _ = write!(output, "\\x{byte:02x}");
            }
        }
    }
    if bytes.len() > maximum {
        output.push('…');
    }
    output
}
