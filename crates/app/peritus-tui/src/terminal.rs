//! Terminal attachment transcript and A3 ordering state.

use std::collections::VecDeque;

use peritus_app_protocol::{
    TerminalBinding, TerminalError, TerminalExit, TerminalInput, TerminalOutput, TerminalPhase,
    TerminalResize, TerminalState,
};

use crate::sanitize::{SafeToken, TerminalSanitizer};

const MAX_TRANSCRIPT_LINES: usize = 10_000;
const MAX_LINE_COLUMNS: usize = 16_384;

/// Local display state for one daemon-owned terminal attachment.
#[derive(Debug)]
pub struct TerminalSession {
    state: TerminalState,
    sanitizer: TerminalSanitizer,
    lines: VecDeque<String>,
    current: Vec<char>,
    cursor: usize,
    capture_input: bool,
    scroll: u16,
}

impl TerminalSession {
    pub(crate) fn new(
        binding: TerminalBinding,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, TerminalError> {
        Ok(Self {
            state: TerminalState::new(binding, maximum_chunk_bytes)?,
            sanitizer: TerminalSanitizer::default(),
            lines: VecDeque::new(),
            current: Vec::new(),
            cursor: 0,
            capture_input: true,
            scroll: 0,
        })
    }

    pub(crate) const fn binding(&self) -> TerminalBinding {
        self.state.binding()
    }

    pub(crate) const fn phase(&self) -> TerminalPhase {
        self.state.phase()
    }

    pub(crate) const fn capture_input(&self) -> bool {
        self.capture_input
    }

    pub(crate) const fn set_capture_input(&mut self, capture: bool) {
        self.capture_input = capture;
    }

    pub(crate) const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub(crate) const fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub(crate) fn resize(&self, resize: TerminalResize) -> Result<(), TerminalError> {
        self.state.resize(resize)
    }

    pub(crate) fn validate_input(&self, input: &TerminalInput) -> Result<(), TerminalError> {
        self.state.accept_input(input)
    }

    pub(crate) fn accept_output(&mut self, output: &TerminalOutput) -> Result<(), TerminalError> {
        self.state.accept_output(output)?;
        for token in self.sanitizer.push(output.bytes()) {
            self.apply_token(token);
        }
        self.scroll = 0;
        Ok(())
    }

    pub(crate) fn accept_exit(&mut self, exit: TerminalExit) -> Result<(), TerminalError> {
        self.state.exit(exit)?;
        if !self.current.is_empty() {
            self.finish_line();
        }
        Ok(())
    }

    pub(crate) fn display_lines(&self) -> Vec<String> {
        let mut lines = self.lines.iter().cloned().collect::<Vec<_>>();
        if !self.current.is_empty() || lines.is_empty() {
            lines.push(self.current.iter().collect());
        }
        lines
    }

    pub(crate) fn visible_lines(&self, height: usize) -> Vec<String> {
        let lines = self.display_lines();
        let end = lines.len().saturating_sub(usize::from(self.scroll));
        let start = end.saturating_sub(height);
        lines[start..end].to_vec()
    }

    fn apply_token(&mut self, token: SafeToken) {
        match token {
            SafeToken::Character(character) => self.write_character(character),
            SafeToken::Newline => self.finish_line(),
            SafeToken::CarriageReturn => self.cursor = 0,
            SafeToken::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    if self.cursor < self.current.len() {
                        self.current.remove(self.cursor);
                    }
                }
            }
            SafeToken::Tab => {
                let spaces = 8 - (self.cursor % 8);
                for _ in 0..spaces {
                    self.write_character(' ');
                }
            }
        }
    }

    fn write_character(&mut self, character: char) {
        if self.cursor >= MAX_LINE_COLUMNS {
            return;
        }
        if self.cursor < self.current.len() {
            self.current[self.cursor] = character;
        } else {
            self.current.push(character);
        }
        self.cursor += 1;
    }

    fn finish_line(&mut self) {
        let line = self.current.iter().collect();
        self.lines.push_back(line);
        while self.lines.len() > MAX_TRANSCRIPT_LINES {
            self.lines.pop_front();
        }
        self.current.clear();
        self.cursor = 0;
    }
}
