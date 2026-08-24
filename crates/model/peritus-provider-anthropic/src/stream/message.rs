//! Anthropic message-level event normalization.

use peritus_model_protocol::{FailureCategory, ModelEvent, ModelName, ResponseId, UsageScope};
use peritus_provider_core::ProviderCoreError;
use serde_json::Value;

use super::state::{NormalizeState, Phase};
use super::value::{
    cache_events, finish_reason, invalid, provider_event, required_str, usage_event,
};
use crate::error::stream_failure;

pub(super) fn start(
    state: &mut NormalizeState,
    value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    if !matches!(state.phase, Phase::AwaitingStart)
        || required_str(value, "/message/type")? != "message"
        || required_str(value, "/message/role")? != "assistant"
        || value.pointer("/message/content").and_then(Value::as_array).is_none_or(|v| !v.is_empty())
    {
        return Err(invalid("Anthropic message_start is out of order or malformed"));
    }
    let response_id = ResponseId::new(required_str(value, "/message/id")?.to_owned())
        .map_err(|_| invalid("Anthropic response identity is invalid"))?;
    let model = ModelName::new(required_str(value, "/message/model")?.to_owned())
        .map_err(|_| invalid("Anthropic response model identity is invalid"))?;
    state.response_id = Some(response_id.clone());
    state.phase = Phase::Content;
    update_usage(state, value.pointer("/message/usage"))?;
    state.emit(
        ModelEvent::ResponseStarted { response_id: Some(response_id), model: Some(model) },
        digest,
        event_id,
    )?;
    state.drain_metadata(digest, event_id)?;
    state.emit(usage_event(&state.usage, UsageScope::Cumulative), digest, event_id)?;
    for event in cache_events(&state.usage) {
        state.emit(event, digest, event_id)?;
    }
    Ok(())
}

pub(super) fn delta(
    state: &mut NormalizeState,
    value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    if !matches!(state.phase, Phase::Content) || !state.blocks.is_empty() {
        return Err(invalid("Anthropic message_delta preceded content completion"));
    }
    let stop = required_str(value, "/delta/stop_reason")?;
    update_usage(state, value.get("usage"))?;
    state.phase = Phase::MessageDelta;
    state.emit(usage_event(&state.usage, UsageScope::Final), digest, event_id)?;
    state.emit(provider_event("anthropic.stop_metadata", value)?, digest, event_id)?;
    state.emit(ModelEvent::Finish(finish_reason(stop)?), digest, event_id)
}

pub(super) fn stop(
    state: &mut NormalizeState,
    _value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    if !matches!(state.phase, Phase::MessageDelta) || !state.blocks.is_empty() {
        return Err(invalid("Anthropic message_stop is out of order"));
    }
    state.phase = Phase::Stopped;
    state.emit(ModelEvent::ResponseCompleted, digest, event_id)
}

pub(super) fn error(
    state: &mut NormalizeState,
    value: &Value,
    digest: peritus_types::Sha256Digest,
    event_id: Option<&str>,
) -> Result<(), ProviderCoreError> {
    let kind = required_str(value, "/error/type")?;
    let category = match kind {
        "authentication_error" => FailureCategory::Authentication,
        "permission_error" => FailureCategory::Permission,
        "rate_limit_error" => FailureCategory::RateLimited,
        "overloaded_error" | "api_error" => FailureCategory::TransientProvider,
        "invalid_request_error" => FailureCategory::InvalidRequest,
        _ => FailureCategory::Provider,
    };
    let failure = stream_failure(
        state.provider.clone(),
        category,
        state.has_observed_semantics(),
        "anthropic.stream.error",
    )?;
    state.emit(ModelEvent::ResponseFailed(failure), digest, event_id)
}

fn update_usage(
    state: &mut NormalizeState,
    value: Option<&Value>,
) -> Result<(), ProviderCoreError> {
    let Some(value) = value else {
        return Ok(());
    };
    update(&mut state.usage.input, value.get("input_tokens"))?;
    update(&mut state.usage.output, value.get("output_tokens"))?;
    update(&mut state.usage.cache_read, value.get("cache_read_input_tokens"))?;
    update(&mut state.usage.cache_creation, value.get("cache_creation_input_tokens"))
}

fn update(target: &mut Option<u64>, value: Option<&Value>) -> Result<(), ProviderCoreError> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = value
        .as_u64()
        .ok_or_else(|| invalid("Anthropic usage counter is not a nonnegative integer"))?;
    if target.is_some_and(|prior| prior > value) {
        return Err(invalid("Anthropic cumulative usage counter regressed"));
    }
    *target = Some(value);
    Ok(())
}
