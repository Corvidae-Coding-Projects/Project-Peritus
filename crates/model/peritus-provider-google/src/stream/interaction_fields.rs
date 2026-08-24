//! Bounded field helpers for the Interactions streaming grammar.

use peritus_model_protocol::ProtocolLimits;
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::value::invalid;

pub(super) fn summary_text(value: &Value) -> Result<&str, ProviderCoreError> {
    let content = value
        .pointer("/delta/content")
        .ok_or_else(|| invalid("Google thought summary content is missing"))?;
    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return Ok(text);
    }
    content
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Google thought summary is not text"))
}

pub(super) fn append_arguments(
    buffer: &mut Vec<u8>,
    bytes: &[u8],
) -> Result<(), ProviderCoreError> {
    let next = buffer
        .len()
        .checked_add(bytes.len())
        .ok_or_else(|| invalid("Google function argument byte count overflowed"))?;
    if next > ProtocolLimits::PRODUCTION.max_tool_argument_bytes() {
        return Err(ProviderCoreError::limit_exceeded(
            "google_stream",
            "Google function arguments exceed their byte bound",
        ));
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn correctness_critical(kind: &str) -> bool {
    kind.starts_with("interaction.") || kind.starts_with("step.")
}
