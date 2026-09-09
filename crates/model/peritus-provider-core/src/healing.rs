//! Bounded syntax-only repair of complete model-authored JSON, never transport framing.
//!
//! The canonical parser and downstream schema/authorization gates remain strict. Repairs do
//! not add values, close truncated containers, rename tools, or retry provider requests.

use peritus_model_protocol::{
    CanonicalJson, ExtensionName, ItemId, JsonBounds, ModelEvent, ProtocolLimits,
    ProviderExtension, StreamFragment, ToolCallId,
};
use serde_json::Value;

use crate::ProviderCoreError;

mod buffer;
mod syntax;
pub use buffer::ToolArgumentBuffer;
#[cfg(test)]
mod tests;

/// Maximum malformed input considered for repair; valid JSON retains its protocol bounds.
pub const MAX_REPAIR_BYTES: usize = 64 * 1024;

/// Strict JSON plus an optional private provenance event.
pub struct HealedJson {
    value: CanonicalJson,
    audit: Option<ModelEvent>,
}

impl HealedJson {
    /// Borrows the strictly validated result.
    #[must_use]
    pub const fn value(&self) -> &CanonicalJson {
        &self.value
    }

    /// Transfers the JSON and optional audit event into the adapter's normalized transcript.
    #[must_use]
    pub fn into_parts(self) -> (CanonicalJson, Option<ModelEvent>) {
        (self.value, self.audit)
    }
}

/// Parses a complete JSON object, repairing only bounded, unambiguous formatting defects.
///
/// `target` identifies the model envelope or tool call in the private response journal.
/// Call only after the provider has closed the payload, never on a partial stream fragment.
///
/// # Errors
/// Rejects nonobjects, duplicate keys, ambiguity, truncation, unsupported syntax and bounds.
pub fn object(
    input: &str,
    target: &str,
    limits: ProtocolLimits,
) -> Result<HealedJson, ProviderCoreError> {
    parse(input, target, limits, true)
}

fn parse(
    input: &str,
    target: &str,
    limits: ProtocolLimits,
    object_required: bool,
) -> Result<HealedJson, ProviderCoreError> {
    let bounds = JsonBounds::value(limits);
    if let Ok(value) = CanonicalJson::parse(input, bounds)
        && (!object_required || value.is_object())
    {
        return Ok(HealedJson { value, audit: None });
    }
    if input.len() > MAX_REPAIR_BYTES || input.len() > bounds.max_bytes() || target.len() > 512 {
        return Err(invalid());
    }
    // Exactly one redundant JSON-string encoding is reversible; never coerce other types.
    let decoded = serde_json::from_str::<String>(input).ok();
    let extracted = syntax::extract(decoded.as_deref().unwrap_or(input)).ok_or_else(invalid)?;
    let repaired = syntax::normalize(extracted).ok_or_else(invalid)?;
    let value = CanonicalJson::parse(&repaired, bounds).map_err(|_| invalid())?;
    if object_required && !value.is_object() {
        return Err(invalid());
    }
    // These are private journal contents like tool arguments, not redacted log diagnostics.
    let record = Value::from_iter([
        ("policy", Value::from("syntax-only-v1")),
        ("target", Value::from(target)),
        ("original", Value::from(input)),
        (
            "repaired",
            Value::from(std::str::from_utf8(value.canonical_bytes()).map_err(|_| invalid())?),
        ),
    ]);
    let audit = CanonicalJson::parse(&record.to_string(), bounds).map_err(|_| invalid())?;
    let name = ExtensionName::new("peritus.response_healing".to_owned()).map_err(|_| invalid())?;
    Ok(HealedJson {
        value,
        audit: Some(ModelEvent::ProviderEvent(ProviderExtension::new(name, audit))),
    })
}

/// Normalizes a closed tool-argument buffer, emitting audit then bounded canonical fragments.
///
/// # Errors
/// Rejects invalid UTF-8 or anything rejected by [`object`]. Tool schema checks still follow.
pub fn tool_arguments(
    bytes: &[u8],
    call_id: &ToolCallId,
    limits: ProtocolLimits,
) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let (value, audit) = object(text, call_id.expose_for_wire(), limits)?.into_parts();
    let mut events: Vec<_> = audit.into_iter().collect();
    for chunk in value.canonical_bytes().chunks(limits.max_event_bytes()) {
        let fragment = StreamFragment::new(chunk.to_vec(), limits).map_err(|_| invalid())?;
        events.push(ModelEvent::ToolArgumentDelta { call_id: call_id.clone(), fragment });
    }
    Ok(events)
}

/// Emits a completed structured response, preserving valid JSON or repairing container syntax.
///
/// Valid scalar/array responses retain their type. Only container-shaped malformed responses
/// are repaired; arbitrary assistant prose and provider framing must not use this function.
///
/// # Errors
/// Rejects invalid UTF-8, bounds, and structured responses that cannot safely be repaired.
pub fn structured_output(
    bytes: &[u8],
    item_id: &ItemId,
    limits: ProtocolLimits,
) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let repaired;
    let mut events = Vec::new();
    let content = if CanonicalJson::parse(text, JsonBounds::value(limits)).is_ok() {
        bytes
    } else {
        let (value, audit) = parse(text, item_id.expose_for_wire(), limits, false)?.into_parts();
        events.extend(audit);
        repaired = value;
        repaired.canonical_bytes()
    };
    for chunk in content.chunks(limits.max_event_bytes()) {
        let fragment = StreamFragment::new(chunk.to_vec(), limits).map_err(|_| invalid())?;
        events.push(ModelEvent::TextDelta { item_id: item_id.clone(), fragment });
    }
    Ok(events)
}

const fn invalid() -> ProviderCoreError {
    ProviderCoreError::malformed_stream(
        "response_healing",
        "model JSON is invalid or cannot be safely repaired within bounds",
    )
}
