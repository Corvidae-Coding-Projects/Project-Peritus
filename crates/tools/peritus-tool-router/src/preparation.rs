//! Router-owned effect-free preparation entry point.

use peritus_tool_protocol::{PreparedToolCall, ToolCall};

use crate::{RouterError, RouterErrorKind, ToolRegistry};

pub fn prepare(registry: &ToolRegistry, call: ToolCall) -> Result<PreparedToolCall, RouterError> {
    let descriptor = registry.descriptor(call.name(), call.version()).ok_or_else(|| {
        RouterError::new(
            RouterErrorKind::Exposure,
            "prepare tool call",
            "tool name/version is unknown or unavailable",
        )
    })?;
    peritus_tool_protocol::prepare_call(descriptor, call).map_err(Into::into)
}
