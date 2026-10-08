//! Chat chunk field validation, usage, and aggregate bounds.

use peritus_model_protocol::{
    OptionalObservationStatus, UsageCounters, UsageObservation, UsageScope,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use crate::error;

pub(super) fn unmapped_top_level(
    value: &Map<String, Value>,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Map<String, Value> {
    value
        .iter()
        .filter(|(name, _)| {
            let generic = matches!(
                name.as_str(),
                "id" | "object"
                    | "created"
                    | "model"
                    | "choices"
                    | "usage"
                    | "system_fingerprint"
                    | "service_tier"
                    | "provider_metadata"
            ) || gateway_metadata(name);
            !generic
                && !matches!(
                    (service, name.as_str()),
                    (Some(peritus_provider_core::hosted::HostedService::Groq), "x_groq")
                        | (
                            Some(peritus_provider_core::hosted::HostedService::OpenRouter),
                            "provider" | "error"
                        )
                )
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub(super) fn gateway_metadata(name: &str) -> bool {
    matches!(
        name,
        "provider_specific_fields"
            | "access_programs"
            | "tool_usage"
            | "frequency_penalty"
            | "presence_penalty"
    )
}

pub(super) fn usage(value: &Value) -> Result<UsageObservation, OptionalObservationStatus> {
    if !value.is_object() {
        return Err(OptionalObservationStatus::InvalidValue);
    }
    let prompt = optional_integer(value, "prompt_tokens")?;
    let completion = optional_integer(value, "completion_tokens")?;
    let total = optional_integer(value, "total_tokens")?;
    if matches!((prompt, completion, total), (Some(a), Some(b), Some(c)) if a.checked_add(b) != Some(c))
    {
        return Err(OptionalObservationStatus::Inconsistent);
    }
    Ok(UsageObservation::new(
        UsageScope::Cumulative,
        UsageCounters::new(prompt, None, None, completion, None, None, total, None),
        None,
    ))
}

pub(super) fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error::malformed("Chat-compatible chunk omitted a required string"))
}

pub(super) fn integer(value: &Value, name: &str) -> Result<u64, ProviderCoreError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| error::malformed("Chat-compatible chunk omitted a required integer"))
}

fn optional_integer(
    value: &Value,
    name: &str,
) -> Result<Option<u64>, OptionalObservationStatus> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or(OptionalObservationStatus::InvalidValue),
    }
}

pub(super) fn append(
    target: &mut Vec<u8>,
    value: &[u8],
    maximum: usize,
) -> Result<(), ProviderCoreError> {
    if target.len().checked_add(value.len()).is_none_or(|length| length > maximum) {
        return Err(error::limit("Chat-compatible fragmented output exceeded aggregate bounds"));
    }
    target.extend_from_slice(value);
    Ok(())
}

/// Validate both semantic choices and content-free final accounting choices.
pub(super) fn validate_choice(
    choice: &Value,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Result<(), ProviderCoreError> {
    let object = choice
        .as_object()
        .ok_or_else(|| error::malformed("Chat-compatible choice was not an object"))?;
    for name in object.keys() {
        let allowed = matches!(name.as_str(), "index" | "delta" | "finish_reason" | "logprobs")
            || (name == "native_finish_reason"
                && service == Some(peritus_provider_core::hosted::HostedService::OpenRouter));
        if !allowed {
            return Err(error::malformed("Chat-compatible choice field was unmapped"));
        }
    }
    Ok(())
}
