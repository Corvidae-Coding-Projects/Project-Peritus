//! Terminal attachment transcript and A3 ordering state.

use peritus_app_protocol::{
    TerminalBinding, TerminalError, TerminalExit, TerminalInput, TerminalOutput, TerminalPhase,
    TerminalResize, TerminalState,
};

mod line_input;
mod transcript;
use transcript::Transcript;

pub fn preview_lines(text: &str) -> Vec<String> {
    let mut transcript = Transcript::default();
    transcript.push(text.as_bytes());
    transcript.finish();
    transcript.display_lines()
}

/// Local display state for one daemon-owned terminal attachment.
#[derive(Debug)]
pub struct TerminalSession {
    state: TerminalState,
    transcript: Transcript,
    capture_input: bool,
    connection_live: bool,
    output_unavailable: bool,
    line_input: Option<line_input::LineInput>,
    maximum_chunk_bytes: usize,
    scroll: usize,
}

impl TerminalSession {
    pub(crate) fn new(
        binding: TerminalBinding,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, TerminalError> {
        Ok(Self {
            state: TerminalState::new(binding, maximum_chunk_bytes)?,
            transcript: Transcript::default(),
            capture_input: true,
            connection_live: true,
            output_unavailable: false,
            line_input: None,
            maximum_chunk_bytes,
            scroll: 0,
        })
    }

    pub(crate) const fn binding(&self) -> TerminalBinding {
        self.state.binding()
    }

    pub(crate) const fn phase(&self) -> TerminalPhase {
        self.state.phase()
    }

    pub(crate) fn phase_label(&self) -> String {
        use peritus_app_protocol::TerminalExitDisposition;
        if self.output_unavailable {
            return "Output unavailable · process may still be running · inspect /preview"
                .to_owned();
        }
        if !self.connection_live && self.phase() == TerminalPhase::Attached {
            return "Connection lost · reconnect and press a to reattach".to_owned();
        }
        match self.phase() {
            TerminalPhase::Attached if self.uses_pipes() => {
                "Attached · line input · Enter sends · Ctrl-C cancels".to_owned()
            }
            TerminalPhase::Attached => "Attached".to_owned(),
            TerminalPhase::Detached(_) => "Detached".to_owned(),
            TerminalPhase::Cancelled(_) => "Cancelled".to_owned(),
            TerminalPhase::Exited(exit) => match exit.disposition() {
                TerminalExitDisposition::Code(code) => format!("Exited · code {code}"),
                TerminalExitDisposition::Signal(signal) => format!("Exited · signal {signal}"),
                TerminalExitDisposition::Unknown => "Exited · status unavailable".to_owned(),
            },
        }
    }

    pub(crate) const fn capture_input(&self) -> bool {
        self.capture_input
    }

    pub(crate) fn set_capture_input(&mut self, capture: bool) {
        self.capture_input = capture && self.can_capture();
    }

    pub(crate) fn can_capture(&self) -> bool {
        self.connection_live && !self.output_unavailable && self.phase() == TerminalPhase::Attached
    }

    pub(crate) fn use_pipes(&mut self) {
        self.line_input = Some(line_input::LineInput::new(self.maximum_chunk_bytes));
    }

    pub(crate) const fn uses_pipes(&self) -> bool {
        self.line_input.is_some()
    }

    pub(crate) fn line_input(&self) -> Option<(&str, usize, bool)> {
        self.line_input.as_ref().map(|line| (line.text.as_str(), line.cursor, line.pending))
    }

    pub(crate) fn settle_line_input(&mut self, accepted: bool) {
        if let Some(line) = &mut self.line_input {
            if accepted && line.pending {
                self.transcript.push(format!("\r\n› {}\r\n", line.text).as_bytes());
            }
            line.settle(accepted);
        }
    }

    pub(crate) const fn output_unavailable(&mut self) {
        self.output_unavailable = true;
        self.capture_input = false;
    }

    pub(crate) const fn disconnect(&mut self) {
        self.connection_live = false;
        self.capture_input = false;
    }

    pub(crate) const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub(crate) const fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub(crate) fn resize(&mut self, resize: TerminalResize) -> Result<(), TerminalError> {
        self.state.resize(resize)?;
        self.transcript.resize(resize.rows(), resize.columns());
        Ok(())
    }

    pub(crate) fn validate_input(&self, input: &TerminalInput) -> Result<(), TerminalError> {
        self.state.accept_input(input)
    }

    pub(crate) fn key_bytes(&mut self, key: crossterm::event::KeyEvent) -> Option<Vec<u8>> {
        if let Some(line) = &mut self.line_input {
            return line.key(key);
        }
        let mut bytes = crate::input::terminal_bytes(key)?;
        if self.transcript.application_cursor()
            && matches!(
                bytes.as_slice(),
                b"\x1b[A" | b"\x1b[B" | b"\x1b[C" | b"\x1b[D" | b"\x1b[H" | b"\x1b[F"
            )
        {
            bytes[1] = b'O';
        }
        Some(bytes)
    }

    pub(crate) fn paste_bytes(&mut self, text: &str) -> Vec<u8> {
        if let Some(line) = &mut self.line_input {
            line.paste(text);
            return Vec::new();
        }
        if self.transcript.bracketed_paste() {
            format!("\x1b[200~{text}\x1b[201~").into_bytes()
        } else {
            text.as_bytes().to_vec()
        }
    }

    pub(crate) fn accept_output(&mut self, output: &TerminalOutput) -> Result<(), TerminalError> {
        self.state.accept_output(output)?;
        if let Some(line) = &mut self.line_input {
            self.transcript.push(&line.display_bytes(output.bytes()));
        } else {
            self.transcript.push(output.bytes());
        }
        self.scroll = 0;
        Ok(())
    }

    pub(crate) fn accept_exit(&mut self, exit: TerminalExit) -> Result<(), TerminalError> {
        self.state.exit(exit)?;
        self.transcript.finish();
        self.capture_input = false;
        Ok(())
    }

    pub(crate) fn visible_lines(&self, height: usize) -> Vec<String> {
        self.transcript.visible_lines(height, self.scroll)
    }

    pub(crate) fn cursor(&self) -> Option<(u16, u16)> {
        (self.capture_input && self.scroll == 0 && self.phase() == TerminalPhase::Attached)
            .then(|| self.transcript.cursor())
            .flatten()
    }
}
