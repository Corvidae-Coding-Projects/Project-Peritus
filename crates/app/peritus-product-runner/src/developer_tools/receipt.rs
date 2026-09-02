//! Durable receipts around mutating developer-tool effects.

mod codec;

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
};

use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::CompletedToolCall;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::path::tool;

const FORMAT_VERSION: u32 = 1;
const MAX_LEDGER_BYTES: usize = 128 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;

pub(super) enum ReceiptDecision {
    Execute,
    Replay { value: Value, is_error: bool },
    Refuse { detail: String, ambiguous: bool },
}

pub(super) struct EffectReceiptLedger {
    path: PathBuf,
    scope: String,
    next_ordinal: u32,
    loaded: bool,
    entries: BTreeMap<u32, ReceiptRecord>,
}

struct ReceiptRecord {
    version: u32,
    scope: String,
    ordinal: u32,
    call_id: String,
    tool: String,
    request_sha256: String,
    state: ReceiptState,
    output: Option<Value>,
    is_error: Option<bool>,
}

enum ReceiptState {
    Started,
    Completed,
    Ambiguous,
}

impl Clone for ReceiptRecord {
    fn clone(&self) -> Self {
        Self {
            version: self.version,
            scope: self.scope.clone(),
            ordinal: self.ordinal,
            call_id: self.call_id.clone(),
            tool: self.tool.clone(),
            request_sha256: self.request_sha256.clone(),
            state: self.state.clone(),
            output: self.output.clone(),
            is_error: self.is_error,
        }
    }
}

impl Clone for ReceiptState {
    fn clone(&self) -> Self {
        match self {
            Self::Started => Self::Started,
            Self::Completed => Self::Completed,
            Self::Ambiguous => Self::Ambiguous,
        }
    }
}

impl EffectReceiptLedger {
    pub(super) const fn new(path: PathBuf, scope: String) -> Self {
        Self { path, scope, next_ordinal: 0, loaded: false, entries: BTreeMap::new() }
    }

    pub(super) fn begin(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<ReceiptDecision, DeveloperLoopError> {
        self.load()?;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| tool("effect receipt ordinal overflowed"))?;
        let ordinal = self.next_ordinal;
        let digest = request_digest(call);
        if let Some(existing) = self.entries.get(&ordinal).cloned() {
            if existing.tool != call.name().as_str() || existing.request_sha256 != digest {
                return Ok(ReceiptDecision::Refuse {
                    detail: format!(
                        "effect receipt conflict at {} effect {}: the recovered request differs from the durably started request",
                        self.scope, ordinal
                    ),
                    ambiguous: false,
                });
            }
            return match existing.state {
                ReceiptState::Completed => Ok(ReceiptDecision::Replay {
                    value: existing
                        .output
                        .ok_or_else(|| tool("completed receipt lost its result"))?,
                    is_error: existing
                        .is_error
                        .ok_or_else(|| tool("completed receipt lost its result status"))?,
                }),
                ReceiptState::Ambiguous => Ok(ReceiptDecision::Refuse {
                    detail: ambiguous(&self.scope, ordinal, &existing.call_id),
                    ambiguous: true,
                }),
                ReceiptState::Started
                    if matches!(
                        call.name().as_str(),
                        "run_command"
                            | "command_start"
                            | "command_stdin"
                            | "command_resize"
                            | "command_signal"
                            | "command_cancel"
                    ) =>
                {
                    let record = ReceiptRecord {
                        state: ReceiptState::Ambiguous,
                        output: None,
                        is_error: None,
                        ..existing
                    };
                    self.append(&record)?;
                    self.entries.insert(ordinal, record.clone());
                    Ok(ReceiptDecision::Refuse {
                        detail: ambiguous(&self.scope, ordinal, &record.call_id),
                        ambiguous: true,
                    })
                }
                ReceiptState::Started => Ok(ReceiptDecision::Execute),
            };
        }
        if self.entries.values().any(|record| {
            record.call_id == call.id().expose_for_wire()
                && (record.tool != call.name().as_str() || record.request_sha256 != digest)
        }) {
            return Ok(ReceiptDecision::Refuse {
                detail: "provider reused one tool-call ID for conflicting effect requests"
                    .to_owned(),
                ambiguous: false,
            });
        }
        let record = ReceiptRecord {
            version: FORMAT_VERSION,
            scope: self.scope.clone(),
            ordinal,
            call_id: call.id().expose_for_wire().to_owned(),
            tool: call.name().as_str().to_owned(),
            request_sha256: digest,
            state: ReceiptState::Started,
            output: None,
            is_error: None,
        };
        self.append(&record)?;
        self.entries.insert(ordinal, record);
        Ok(ReceiptDecision::Execute)
    }

    pub(super) fn complete(
        &mut self,
        value: &Value,
        is_error: bool,
    ) -> Result<(), DeveloperLoopError> {
        let ordinal = self.next_ordinal;
        let existing = self
            .entries
            .get(&ordinal)
            .cloned()
            .ok_or_else(|| tool("effect completed without a started receipt"))?;
        if !matches!(existing.state, ReceiptState::Started) {
            return Err(tool("effect receipt is not awaiting completion"));
        }
        let record = ReceiptRecord {
            state: ReceiptState::Completed,
            output: Some(value.clone()),
            is_error: Some(is_error),
            ..existing
        };
        self.append(&record)?;
        self.entries.insert(ordinal, record);
        Ok(())
    }

    fn load(&mut self) -> Result<(), DeveloperLoopError> {
        if self.loaded {
            return Ok(());
        }
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.loaded = true;
                return Ok(());
            }
            Err(error) => return Err(tool(format!("read effect receipts: {error}"))),
        };
        if bytes.len() > MAX_LEDGER_BYTES {
            return Err(tool("effect receipt ledger exceeds its byte bound"));
        }
        let mut offset = 0_usize;
        while bytes.len().saturating_sub(offset) >= 8 {
            let length = u64::from_le_bytes(
                bytes[offset..offset + 8]
                    .try_into()
                    .map_err(|_| tool("effect receipt length is malformed"))?,
            );
            let length = usize::try_from(length)
                .map_err(|_| tool("effect receipt length exceeds this platform"))?;
            if length > MAX_RECORD_BYTES {
                return Err(tool("effect receipt record exceeds its byte bound"));
            }
            let start = offset + 8;
            let Some(end) = start.checked_add(length) else {
                return Err(tool("effect receipt length overflowed"));
            };
            if end > bytes.len() {
                break;
            }
            let value: Value = serde_json::from_slice(&bytes[start..end])
                .map_err(|error| tool(format!("decode effect receipt: {error}")))?;
            let record = codec::decode(&value)?;
            self.accept_loaded(record)?;
            offset = end;
        }
        self.loaded = true;
        Ok(())
    }

    fn accept_loaded(&mut self, record: ReceiptRecord) -> Result<(), DeveloperLoopError> {
        if record.version != FORMAT_VERSION || record.scope != self.scope {
            return Ok(());
        }
        if let Some(previous) = self.entries.get(&record.ordinal)
            && (previous.tool != record.tool
                || previous.request_sha256 != record.request_sha256
                || previous.call_id != record.call_id)
        {
            return Err(tool("effect receipt history contains a conflicting action identity"));
        }
        self.entries.insert(record.ordinal, record);
        Ok(())
    }

    fn append(&self, record: &ReceiptRecord) -> Result<(), DeveloperLoopError> {
        let payload = serde_json::to_vec(&codec::encode(record))
            .map_err(|error| tool(format!("encode effect receipt: {error}")))?;
        if payload.len() > MAX_RECORD_BYTES {
            return Err(tool("effect receipt result exceeds its byte bound"));
        }
        let frame_bytes = payload
            .len()
            .checked_add(8)
            .ok_or_else(|| tool("effect receipt frame length overflowed"))?;
        let retained =
            fs::metadata(&self.path)
                .map(|metadata| metadata.len())
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound { Ok(0) } else { Err(error) }
                })
                .map_err(|error| tool(format!("inspect effect receipts: {error}")))?;
        let frame_bytes = u64::try_from(frame_bytes)
            .map_err(|_| tool("effect receipt frame exceeds this platform"))?;
        if retained.checked_add(frame_bytes).is_none_or(|total| total > MAX_LEDGER_BYTES as u64) {
            return Err(tool("effect receipt ledger exceeds its byte bound"));
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| tool(format!("create effect receipt directory: {error}")))?;
        }
        let length = u64::try_from(payload.len())
            .map_err(|_| tool("effect receipt result exceeds this platform"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| tool(format!("open effect receipts: {error}")))?;
        file.write_all(&length.to_le_bytes())
            .and_then(|()| file.write_all(&payload))
            .and_then(|()| file.sync_data())
            .map_err(|error| tool(format!("persist effect receipt: {error}")))
    }
}

fn request_digest(call: &CompletedToolCall) -> String {
    let mut hasher = Sha256::new();
    hasher.update(call.name().as_str().as_bytes());
    hasher.update([0]);
    hasher.update(call.arguments().canonical_bytes());
    hex(hasher.finalize().into())
}

fn hex(bytes: [u8; 32]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn ambiguous(scope: &str, ordinal: u32, call_id: &str) -> String {
    format!(
        "ambiguous prior command outcome at {scope} effect {ordinal} (provider call {call_id}); Peritus will not run it again automatically because the command may already have taken effect"
    )
}

#[cfg(test)]
mod tests {
    use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits, ToolCallId, ToolName};

    use super::*;

    #[test]
    fn completed_effect_replays_and_conflicting_request_is_refused() {
        let directory = tempfile::tempdir().expect("state");
        let path = directory.path().join("effects.bin");
        let original = call("call-1", "workspace_write", r#"{"content":"one","path":"a"}"#);
        let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
        assert!(matches!(first.begin(&original).expect("start"), ReceiptDecision::Execute));
        let mut output = serde_json::Map::new();
        output.insert("changed".to_owned(), Value::Bool(true));
        first.complete(&Value::Object(output), false).expect("complete");

        let mut replay = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
        assert!(matches!(
            replay.begin(&original).expect("replay"),
            ReceiptDecision::Replay { is_error: false, .. }
        ));
        let conflicting = call("call-2", "workspace_write", r#"{"content":"two","path":"a"}"#);
        let mut conflict = EffectReceiptLedger::new(path, "writer-1".to_owned());
        assert!(matches!(
            conflict.begin(&conflicting).expect("conflict"),
            ReceiptDecision::Refuse { detail, ambiguous: false } if detail.contains("differs")
        ));
    }

    #[test]
    fn interrupted_command_is_durably_ambiguous_and_never_relaunched() {
        let directory = tempfile::tempdir().expect("state");
        let path = directory.path().join("effects.bin");
        let call = call("call-1", "run_command", r#"{"args":[],"program":"example"}"#);
        let mut first = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
        assert!(matches!(first.begin(&call).expect("start"), ReceiptDecision::Execute));

        for _ in 0..2 {
            let mut recovered = EffectReceiptLedger::new(path.clone(), "writer-1".to_owned());
            assert!(matches!(
                recovered.begin(&call).expect("recover"),
                ReceiptDecision::Refuse { detail, ambiguous: true }
                    if detail.contains("ambiguous prior command outcome")
            ));
        }
    }

    fn call(id: &str, name: &str, arguments: &str) -> CompletedToolCall {
        CompletedToolCall::new(
            ToolCallId::new(id.to_owned()).expect("call ID"),
            ToolName::new(name.to_owned()).expect("tool name"),
            CanonicalJson::parse(arguments, JsonBounds::value(ProtocolLimits::PRODUCTION))
                .expect("arguments"),
        )
        .expect("completed call")
    }
}
