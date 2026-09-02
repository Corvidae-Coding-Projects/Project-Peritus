//! Canonical JSON observations and bounded argument decoding for developer tools.

use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits};
use serde_json::{Map, Value};

use super::path::tool;

pub(super) fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect::<Map<_, _>>(),
    )
}

pub(super) fn collection(name: &str, values: Vec<Value>, truncated: bool) -> Value {
    object(vec![(name, Value::Array(values)), ("truncated", Value::Bool(truncated))])
}

pub(super) fn observation(
    value: &Value,
    is_error: bool,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let encoded = serde_json::to_string(value).map_err(|error| tool(error.to_string()))?;
    let output = CanonicalJson::parse(&encoded, JsonBounds::value(ProtocolLimits::PRODUCTION))?;
    Ok(DeveloperToolObservation { output, is_error })
}

pub(super) fn required_string<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a str, DeveloperLoopError> {
    string(value, name).ok_or_else(|| tool(format!("{name} must be text")))
}

pub(super) fn string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}

pub(super) fn bounded_usize(
    value: &Value,
    name: &str,
    default: usize,
    minimum: usize,
    maximum: usize,
) -> usize {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

pub(super) fn bounded_u64(
    value: &Value,
    name: &str,
    default: u64,
    minimum: u64,
    maximum: u64,
) -> u64 {
    value.get(name).and_then(Value::as_u64).unwrap_or(default).clamp(minimum, maximum)
}
