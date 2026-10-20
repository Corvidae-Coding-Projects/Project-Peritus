use std::io::{Seek as _, Write as _};

use serde_json::Value;

use crate::error::CliError;

pub struct Output {
    json: bool,
}

impl Output {
    pub(crate) const fn new(json: bool) -> Self {
        Self { json }
    }

    pub(crate) fn success(
        &self,
        kind: &str,
        value: impl Into<Value>,
        human: &str,
    ) -> Result<(), CliError> {
        if self.json {
            let value = value.into();
            let document = serde_json::json!({ "ok": true, "kind": kind, "result": value });
            Self::line(&document.to_string())
        } else {
            Self::line(human)
        }
    }

    pub(crate) fn event(&self, value: impl Into<Value>, human: &str) -> Result<(), CliError> {
        if self.json { Self::line(&value.into().to_string()) } else { Self::line(human) }
    }

    /// Render one large success document to an automatically removed file, then stream it out.
    /// A producer error publishes no success prefix or partial JSON. Memory stays bounded by
    /// the producer's current record and the I/O buffer rather than the complete result.
    pub(crate) fn success_stream(
        &self,
        kind: &str,
        render: impl FnOnce(&mut dyn std::io::Write, bool) -> Result<(), CliError>,
    ) -> Result<(), CliError> {
        let mut file = tempfile::tempfile()
            .map_err(|error| CliError::local_io("create temporary streamed output", None, error))?;
        {
            let mut writer = std::io::BufWriter::new(&mut file);
            if self.json {
                writer.write_all(b"{\"ok\":true,\"kind\":").map_err(CliError::output)?;
                serde_json::to_writer(&mut writer, kind)
                    .map_err(|error| CliError::output(std::io::Error::other(error)))?;
                writer.write_all(b",\"result\":").map_err(CliError::output)?;
            }
            render(&mut writer, self.json)?;
            if self.json {
                writer.write_all(b"}").map_err(CliError::output)?;
            }
            writer.write_all(b"\n").map_err(CliError::output)?;
            writer.flush().map_err(CliError::output)?;
        }
        file.rewind().map_err(CliError::output)?;
        let mut stdout = std::io::stdout().lock();
        std::io::copy(&mut file, &mut stdout).map_err(CliError::output)?;
        stdout.flush().map_err(CliError::output)
    }

    pub(crate) fn terminal_bytes(
        &self,
        value: impl Into<Value>,
        bytes: &[u8],
        sanitizer: &mut TerminalSanitizer,
    ) -> Result<(), CliError> {
        if self.json {
            Self::line(&value.into().to_string())
        } else {
            let safe = sanitizer.sanitize(bytes);
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(safe.as_bytes()).map_err(CliError::output)?;
            stdout.flush().map_err(CliError::output)
        }
    }

    fn line(value: &str) -> Result<(), CliError> {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(value.as_bytes()).map_err(CliError::output)?;
        stdout.write_all(b"\n").map_err(CliError::output)?;
        stdout.flush().map_err(CliError::output)
    }
}

#[derive(Default)]
pub struct TerminalSanitizer {
    escape: EscapeState,
}

#[derive(Clone, Copy, Default)]
enum EscapeState {
    #[default]
    Text,
    Escape,
    ControlSequence,
    OperatingSystemCommand,
    OperatingSystemEscape,
}

impl TerminalSanitizer {
    pub(crate) fn sanitize(&mut self, bytes: &[u8]) -> String {
        let mut safe = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            match self.escape {
                EscapeState::Text => match byte {
                    0x1b => self.escape = EscapeState::Escape,
                    b'\n' | b'\r' | b'\t' | 0x20..=0x7e | 0x80..=0xff => safe.push(byte),
                    _ => safe.extend_from_slice(b"?"),
                },
                EscapeState::Escape => match byte {
                    b'[' => self.escape = EscapeState::ControlSequence,
                    b']' | b'P' | b'X' | b'^' | b'_' => {
                        self.escape = EscapeState::OperatingSystemCommand;
                    }
                    _ => self.escape = EscapeState::Text,
                },
                EscapeState::ControlSequence => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.escape = EscapeState::Text;
                    }
                }
                EscapeState::OperatingSystemCommand => match byte {
                    0x07 => self.escape = EscapeState::Text,
                    0x1b => self.escape = EscapeState::OperatingSystemEscape,
                    _ => {}
                },
                EscapeState::OperatingSystemEscape => {
                    self.escape = if byte == b'\\' {
                        EscapeState::Text
                    } else {
                        EscapeState::OperatingSystemCommand
                    };
                }
            }
        }
        String::from_utf8_lossy(&safe).into_owned()
    }
}
