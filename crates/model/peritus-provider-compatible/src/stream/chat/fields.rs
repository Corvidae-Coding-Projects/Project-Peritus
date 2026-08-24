//! Chat chunk field validation, usage, and aggregate bounds.

use peritus_model_protocol::{UsageCounters, UsageObservation, UsageScope};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use crate::error;

pub(super) fn validate_top_level(value: &Map<String, Value>) -> Result<(), ProviderCoreError> {
    for name in value.keys() {
        if !matches!(
            name.as_str(),
            "id" | "object"
                | "created"
                | "model"
                | "choices"
                | "usage"
                | "system_fingerprint"
                | "service_tier"
                | "provider_metadata"
        ) {
            return Err(error::malformed("Chat-compatible top-level field was unmapped"));
        }
    }
    Ok(())
}

pub(super) fn usage(value: &Value) -> Result<UsageObservation, ProviderCoreError> {
    let prompt = optional_integer(value, "prompt_tokens")?;
    let completion = optional_integer(value, "completion_tokens")?;
    let total = optional_integer(value, "total_tokens")?;
    if matches!((prompt, completion, total), (Some(a), Some(b), Some(c)) if a.checked_add(b) != Some(c))
    {
        return Err(error::malformed("Chat-compatible usage total was inconsistent"));
    }
    Ok(UsageObservation::new(
        UsageScope::Final,
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

fn optional_integer(value: &Value, name: &str) -> Result<Option<u64>, ProviderCoreError> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| error::malformed("Chat-compatible usage counter was invalid")),
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
