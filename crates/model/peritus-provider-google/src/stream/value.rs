//! Checked Google stream identities, fragments, usage, cache, and ancillary observations.

use peritus_model_protocol::{
    CacheObservation, CacheStatus, CanonicalJson, ExtensionName, ItemId, JsonBounds, ModelEvent,
    ProtocolLimits, ProviderExtension, StreamFragment, ToolCallId, ToolName, UsageCounters,
    UsageObservation, UsageScope,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError};
use serde_json::{Map, Value};

pub(super) const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::malformed_stream("google_stream", detail)
}

pub(super) fn required_str<'a>(
    value: &'a Value,
    pointer: &str,
) -> Result<&'a str, ProviderCoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Google event omitted a required bounded string"))
}

pub(super) fn required_u32(value: &Value, pointer: &str) -> Result<u32, ProviderCoreError> {
    let value = value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("Google event omitted a required index"))?;
    u32::try_from(value).map_err(|_| invalid("Google event index exceeds u32"))
}

pub(super) fn item_id(prefix: &str, index: u32) -> Result<ItemId, ProviderCoreError> {
    ItemId::new(format!("google-{prefix}-{index}"))
        .map_err(|_| invalid("Google item identity is invalid"))
}

pub(super) fn call_id(value: &str) -> Result<ToolCallId, ProviderCoreError> {
    ToolCallId::new(value.to_owned()).map_err(|_| invalid("Google tool-call identity is invalid"))
}

pub(super) fn tool_name(value: &str) -> Result<ToolName, ProviderCoreError> {
    ToolName::new(value.to_owned()).map_err(|_| invalid("Google tool name is invalid"))
}

pub(super) fn fragment(bytes: Vec<u8>) -> Result<StreamFragment, ProviderCoreError> {
    StreamFragment::new(bytes, ProtocolLimits::PRODUCTION)
        .map_err(|_| invalid("Google stream fragment exceeds its bound"))
}

pub(super) fn provider_event(name: &str, value: &Value) -> Result<ModelEvent, ProviderCoreError> {
    if name.is_empty()
        || name.len() > 96
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid("Google ancillary event name is unsafe"));
    }
    let name = ExtensionName::new(format!("google.{name}"))
        .map_err(|_| invalid("Google ancillary event name is invalid"))?;
    let bytes = serde_json::to_vec(value)
        .map_err(|_| invalid("Google ancillary event could not be serialized"))?;
    let value = CanonicalJson::parse(
        core::str::from_utf8(&bytes).map_err(|_| invalid("Google ancillary event is not UTF-8"))?,
        JsonBounds::value(ProtocolLimits::PRODUCTION),
    )
    .map_err(|_| invalid("Google ancillary event exceeds JSON bounds"))?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
}

pub(super) fn usage(
    value: &Value,
    scope: UsageScope,
    interactions: bool,
) -> Result<ModelEvent, ProviderCoreError> {
    let get = |interaction: &str, generate: &str| {
        value
            .get(if interactions { interaction } else { generate })
            .map(|field| {
                field
                    .as_u64()
                    .ok_or_else(|| invalid("Google usage counter is not an unsigned integer"))
            })
            .transpose()
    };
    let input = get("total_input_tokens", "promptTokenCount")?;
    let cached = get("total_cached_tokens", "cachedContentTokenCount")?;
    let output = get("total_output_tokens", "candidatesTokenCount")?;
    let thoughts = get("total_thought_tokens", "thoughtsTokenCount")?;
    let tools = get("total_tool_use_tokens", "toolUsePromptTokenCount")?;
    let total = get("total_tokens", "totalTokenCount")?;
    Ok(ModelEvent::Usage(UsageObservation::new(
        scope,
        UsageCounters::new(input, cached, None, output, thoughts, tools, total, None),
        None,
    )))
}

pub(super) fn cache(value: &Value, interactions: bool) -> Option<ModelEvent> {
    let key = if interactions { "total_cached_tokens" } else { "cachedContentTokenCount" };
    value.get(key).and_then(Value::as_u64).filter(|tokens| *tokens > 0).map(|tokens| {
        ModelEvent::Cache(CacheObservation::new(CacheStatus::Hit, None, Some(tokens), None))
    })
}

pub(super) fn metadata_events(headers: &HttpHeaders) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let mut events = Vec::new();
    for name in ["x-request-id", "x-goog-request-id"] {
        if let Some(value) = headers
            .first(name)
            .and_then(|header| header.nonsensitive_bytes())
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
        {
            let mut metadata = Map::new();
            metadata.insert("request_id".to_owned(), Value::String(value.to_owned()));
            events.push(provider_event("request_metadata", &Value::Object(metadata))?);
            break;
        }
    }
    Ok(events)
}
