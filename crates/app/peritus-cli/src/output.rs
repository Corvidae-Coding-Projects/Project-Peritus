use std::io::Write as _;

use serde_json::Value;

use crate::error::CliError;

const STREAM_OUTPUT_CAPACITY: usize = 8;

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

    pub(crate) fn event_record(&self, value: impl Into<Value>, human: &str) -> OutputRecord {
        let mut bytes = if self.json {
            value.into().to_string().into_bytes()
        } else {
            human.as_bytes().to_vec()
        };
        bytes.push(b'\n');
        OutputRecord { bytes }
    }

    pub(crate) fn success_record(
        &self,
        kind: &str,
        value: impl Into<Value>,
        human: &str,
    ) -> OutputRecord {
        if self.json {
            let document =
                serde_json::json!({ "ok": true, "kind": kind, "result": value.into() });
            self.event_record(document, human)
        } else {
            self.event_record(Value::Null, human)
        }
    }

    pub(crate) fn stream(&self) -> Result<StreamOutput, CliError> {
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<OwnedOutput>(STREAM_OUTPUT_CAPACITY);
        std::thread::Builder::new()
            .name("peritus-cli-output".to_owned())
            .spawn(move || {
                let mut stdout = std::io::stdout().lock();
                while let Some(output) = receiver.blocking_recv() {
                    let result = stdout
                        .write_all(&output.record.bytes)
                        .and_then(|()| stdout.flush());
                    let failed = result.is_err();
                    let _ = output.completed.send(result);
                    if failed {
                        break;
                    }
                }
            })
            .map_err(CliError::output)?;
        Ok(StreamOutput { sender })
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

pub(crate) struct OutputRecord {
    bytes: Vec<u8>,
}

struct OwnedOutput {
    record: OutputRecord,
    completed: tokio::sync::oneshot::Sender<Result<(), std::io::Error>>,
}

#[derive(Clone)]
pub(crate) struct StreamOutput {
    sender: tokio::sync::mpsc::Sender<OwnedOutput>,
}

impl StreamOutput {
    pub(crate) async fn write(&self, record: OutputRecord) -> Result<(), CliError> {
        let (completed, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(OwnedOutput { record, completed })
            .await
            .map_err(|_| {
                CliError::runtime("queue stream output", "output worker is unavailable")
            })?;
        receiver
            .await
            .map_err(|_| {
                CliError::runtime("await stream output", "output worker stopped unexpectedly")
            })?
            .map_err(CliError::output)
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
    pub(crate) fn discontinuity(&mut self) {
        self.escape = EscapeState::Text;
    }

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
