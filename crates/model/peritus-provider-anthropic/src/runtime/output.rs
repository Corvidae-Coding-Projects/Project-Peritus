//! Fail-closed decoding of Claude's final structured result envelope.

use std::collections::BTreeSet;

use peritus_model_protocol::UsageCounters;
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeFailure {
    Reported,
    Incomplete,
    Malformed,
}

pub(super) struct RuntimeTurn {
    pub content: String,
    pub tool_calls: Vec<RuntimeToolCall>,
    pub usage: UsageCounters,
}

pub(super) struct RuntimeToolCall {
    pub name: String,
    pub arguments: Map<String, Value>,
}

pub(super) fn decode(
    bytes: &[u8],
    allowed_tools: &BTreeSet<String>,
    max_calls: usize,
) -> Result<RuntimeTurn, DecodeFailure> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| DecodeFailure::Malformed)?;
    let raw = value.as_object().ok_or(DecodeFailure::Malformed)?;
    if optional_bool(raw, "is_error")?.unwrap_or(false) {
        return Err(DecodeFailure::Reported);
    }
    let turn = raw
        .get("structured_output")
        .or_else(|| raw.get("structuredOutput"))
        .ok_or(DecodeFailure::Incomplete)?
        .as_object()
        .ok_or(DecodeFailure::Malformed)?;
    let content = required_string(turn, "content")?;
    if content.is_empty() || content.contains('\0') {
        return Err(DecodeFailure::Malformed);
    }
    let calls = turn.get("tool_calls").and_then(Value::as_array).ok_or(DecodeFailure::Malformed)?;
    if calls.len() > max_calls {
        return Err(DecodeFailure::Malformed);
    }
    let mut tool_calls = Vec::with_capacity(calls.len());
    for call in calls {
        let call = call.as_object().ok_or(DecodeFailure::Malformed)?;
        let name = required_string(call, "name")?;
        if name.trim() != name || name.is_empty() || !allowed_tools.contains(name) {
            return Err(DecodeFailure::Malformed);
        }
        let arguments = call
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or(DecodeFailure::Malformed)?
            .clone();
        tool_calls.push(RuntimeToolCall { name: name.to_owned(), arguments });
    }
    Ok(RuntimeTurn { content: content.to_owned(), tool_calls, usage: usage(raw)? })
}

fn usage(raw: &Map<String, Value>) -> Result<UsageCounters, DecodeFailure> {
    let Some(value) = raw.get("usage") else {
        return Ok(UsageCounters::new(None, None, None, None, None, None, None, None));
    };
    let usage = value.as_object().ok_or(DecodeFailure::Malformed)?;
    Ok(UsageCounters::new(
        optional_u64(usage, "input_tokens")?,
        optional_u64(usage, "cache_read_input_tokens")?,
        optional_u64(usage, "cache_creation_input_tokens")?,
        optional_u64(usage, "output_tokens")?,
        None,
        None,
        None,
        None,
    ))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    name: &str,
) -> Result<&'a str, DecodeFailure> {
    object.get(name).and_then(Value::as_str).ok_or(DecodeFailure::Malformed)
}

fn optional_bool(object: &Map<String, Value>, name: &str) -> Result<Option<bool>, DecodeFailure> {
    object.get(name).map(|value| value.as_bool().ok_or(DecodeFailure::Malformed)).transpose()
}

fn optional_u64(object: &Map<String, Value>, name: &str) -> Result<Option<u64>, DecodeFailure> {
    object.get(name).map(|value| value.as_u64().ok_or(DecodeFailure::Malformed)).transpose()
}
