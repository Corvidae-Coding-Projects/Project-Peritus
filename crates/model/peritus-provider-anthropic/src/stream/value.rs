//! Bounded JSON extraction and normalized metadata helpers.

use peritus_model_protocol::{
    BoundedText, CacheObservation, CacheStatus, CanonicalJson, ExtensionName, FinishReason, ItemId,
    JsonBounds, ModelEvent, ProtocolLimits, ProviderExtension, RateLimitDimension,
    RateLimitObservation, RateLimitWindow, UsageCounters, UsageObservation, UsageScope,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError};
use serde_json::{Map, Value};

use super::state::UsageState;

pub(super) const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::malformed_stream("anthropic_stream", detail)
}

pub(super) fn required_str<'a>(
    value: &'a Value,
    pointer: &str,
) -> Result<&'a str, ProviderCoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Anthropic event string field is missing or invalid"))
}

pub(super) fn required_u32(value: &Value, pointer: &str) -> Result<u32, ProviderCoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| invalid("Anthropic event index is missing or out of range"))
}

pub(super) fn item_id(
    response_id: &peritus_model_protocol::ResponseId,
    index: u32,
) -> Result<ItemId, ProviderCoreError> {
    ItemId::new(format!("{}:{index}", response_id.expose_for_wire()))
        .map_err(|_| invalid("Anthropic-derived item identity is invalid"))
}

pub(super) fn usage_event(usage: &UsageState, scope: UsageScope) -> ModelEvent {
    let total = usage.input.zip(usage.output).and_then(|(input, output)| input.checked_add(output));
    ModelEvent::Usage(UsageObservation::new(
        scope,
        UsageCounters::new(
            usage.input,
            usage.cache_read,
            usage.cache_creation,
            usage.output,
            None,
            None,
            total,
            None,
        ),
        None,
    ))
}

pub(super) fn cache_events(usage: &UsageState) -> Vec<ModelEvent> {
    let mut events = Vec::new();
    if usage.cache_creation.unwrap_or(0) > 0 {
        events.push(ModelEvent::Cache(CacheObservation::new(
            CacheStatus::Created,
            None,
            usage.cache_creation,
            None,
        )));
    }
    if usage.cache_read.unwrap_or(0) > 0 {
        events.push(ModelEvent::Cache(CacheObservation::new(
            CacheStatus::Hit,
            None,
            usage.cache_read,
            None,
        )));
    }
    events
}

pub(super) fn finish_reason(raw: &str) -> Result<FinishReason, ProviderCoreError> {
    Ok(match raw {
        "end_turn" | "stop_sequence" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolCalls,
        "pause_turn" => FinishReason::Pause,
        "refusal" => FinishReason::Refusal,
        "model_context_window_exceeded" => FinishReason::ContextLimit,
        unknown => FinishReason::Provider(
            BoundedText::new(unknown.to_owned(), ProtocolLimits::PRODUCTION)
                .map_err(|_| invalid("Anthropic stop reason exceeds protocol bounds"))?,
        ),
    })
}

pub(super) fn provider_event(name: &str, value: &Value) -> Result<ModelEvent, ProviderCoreError> {
    let name = ExtensionName::new(name.to_owned())
        .map_err(|_| invalid("Anthropic provider event name is invalid"))?;
    let bytes = serde_json::to_string(value)
        .map_err(|_| invalid("Anthropic provider event serialization failed"))?;
    let value = CanonicalJson::parse(&bytes, JsonBounds::value(ProtocolLimits::PRODUCTION))
        .map_err(|_| invalid("Anthropic provider event exceeds JSON bounds"))?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
}

pub(super) fn replay_fragment(
    kind: &str,
    field: &str,
    bytes: &str,
) -> Result<Vec<u8>, ProviderCoreError> {
    let mut replay = Map::new();
    replay.insert("type".to_owned(), Value::String(kind.to_owned()));
    replay.insert(field.to_owned(), Value::String(bytes.to_owned()));
    serde_json::to_vec(&Value::Object(replay))
        .map_err(|_| invalid("Anthropic reasoning replay serialization failed"))
}

pub(super) fn metadata_events(headers: &HttpHeaders) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let mut events = Vec::new();
    if let Some(request_id) =
        header_text(headers, "request-id").or_else(|| header_text(headers, "x-request-id"))
    {
        let mut metadata = Map::new();
        metadata.insert("request_id".to_owned(), Value::String(request_id.to_owned()));
        events.push(provider_event("anthropic.request_metadata", &Value::Object(metadata))?);
    }
    let dimensions = [
        (
            RateLimitDimension::Requests,
            "anthropic-ratelimit-requests-limit",
            "anthropic-ratelimit-requests-remaining",
        ),
        (
            RateLimitDimension::InputTokens,
            "anthropic-ratelimit-input-tokens-limit",
            "anthropic-ratelimit-input-tokens-remaining",
        ),
        (
            RateLimitDimension::OutputTokens,
            "anthropic-ratelimit-output-tokens-limit",
            "anthropic-ratelimit-output-tokens-remaining",
        ),
    ];
    let mut windows = Vec::new();
    for (dimension, limit_name, remaining_name) in dimensions {
        let limit = header_u64(headers, limit_name);
        let remaining = header_u64(headers, remaining_name);
        if limit.is_some() || remaining.is_some() {
            windows.push(
                RateLimitWindow::new(dimension, limit, remaining, None)
                    .map_err(|_| invalid("Anthropic rate-limit headers are inconsistent"))?,
            );
        }
    }
    if !windows.is_empty() {
        events.push(ModelEvent::RateLimit(
            RateLimitObservation::new(windows)
                .map_err(|_| invalid("Anthropic rate-limit metadata exceeds bounds"))?,
        ));
    }
    Ok(events)
}

fn header_u64(headers: &HttpHeaders, name: &str) -> Option<u64> {
    header_text(headers, name)?.parse().ok()
}

fn header_text<'a>(headers: &'a HttpHeaders, name: &str) -> Option<&'a str> {
    core::str::from_utf8(headers.first(name)?.nonsensitive_bytes()?).ok()
}
