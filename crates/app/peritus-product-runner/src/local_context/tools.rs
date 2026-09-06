//! Bounded memory tools; the underlying executor retains every authority and grounding gate.

mod entry_view;
pub(super) mod read;
mod schema;
mod secret_text;
pub(super) mod update;

use super::{LocalContextHandle, error, memory::LocalMemory};
use peritus_agent::{DeveloperLoopError, DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::{CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits};
use serde_json::Value;

pub(super) use entry_view::entry_view;
pub use schema::definitions;

pub struct MemoryTools<'a> {
    base: &'a mut dyn DeveloperToolExecutor,
    memory: LocalContextHandle,
}

impl<'a> MemoryTools<'a> {
    pub(crate) const fn new(
        base: &'a mut dyn DeveloperToolExecutor,
        memory: LocalContextHandle,
    ) -> Self {
        Self { base, memory }
    }
}

impl DeveloperToolExecutor for MemoryTools<'_> {
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let name = call.name().as_str();
        if !matches!(name, "context_read" | "context_update") {
            return self.base.execute(call);
        }
        // No lock crosses base execution, and memory never updates its grounding/receipt state.
        let mut memory = self.memory.lock()?;
        let value = match name {
            "context_read" => read::execute(&mut memory, call.arguments().canonical_bytes())?,
            _ => update::execute(&mut memory, call.arguments().canonical_bytes())?,
        };
        drop(memory);
        let is_error = value.get("rejected").is_some();
        let output = CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?;
        Ok(DeveloperToolObservation { output, is_error })
    }
    fn completion_blocker(&self) -> Option<String> {
        self.base.completion_blocker()
    }
    fn required_tool_name(&self) -> Option<&str> {
        self.base.required_tool_name()
    }
    fn take_progress_feedback(&mut self) -> Option<String> {
        self.base.take_progress_feedback()
    }
}

pub(super) fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

pub(super) fn source_handle(memory: &LocalMemory, sequence: u64) -> String {
    format!("obs:{}:{sequence:06}", hex(memory.store.scope_digest().as_bytes()))
}

use source_handle as handle;

fn sequence(memory: &LocalMemory, handle: &str) -> Result<u64, DeveloperLoopError> {
    let tail = handle.strip_prefix("obs:").ok_or_else(|| error("invalid observation handle"))?;
    let number = if let Some((scope, number)) = tail.split_once(':') {
        if scope != hex(memory.store.scope_digest().as_bytes()) {
            return Err(error("observation scope mismatch"));
        }
        number
    } else {
        tail
    };
    if number.is_empty() || number.len() > 20 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error("invalid observation number"));
    }
    let sequence = number.parse().map_err(|_| error("invalid observation number"))?;
    memory.archived(sequence)?;
    Ok(sequence)
}

fn rejected(memory: &LocalMemory, reason: &str) -> Value {
    Value::from_iter([
        ("rejected", Value::from(reason)),
        ("base_revision", Value::from(memory.model_revision)),
        ("authority", Value::from("none")),
    ])
}
