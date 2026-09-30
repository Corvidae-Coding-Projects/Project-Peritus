//! Fail-closed decoding of Claude's final structured result envelope.

use std::collections::BTreeSet;

use peritus_model_protocol::{
    CanonicalJson, JsonBounds, ModelEvent, ProtocolLimits, UsageCounters,
};
use serde_json::{Map, Value};

mod completion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeFailure {
    Authentication,
    Capacity,
    ContextLimit,
    InvalidModel,
    Reported,
    Incomplete,
    Malformed,
}

pub(super) struct RuntimeTurn {
    pub content: String,
    pub tool_calls: Vec<RuntimeToolCall>,
    pub usage: UsageCounters,
    pub repairs: Vec<ModelEvent>,
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
    let text = std::str::from_utf8(bytes).map_err(|_| DecodeFailure::Malformed)?;
    // Validate the runtime transport verbatim, including duplicate keys. Never heal it.
    CanonicalJson::parse(text, JsonBounds::value(ProtocolLimits::PRODUCTION))
        .map_err(|_| DecodeFailure::Malformed)?;
    let value: Value = serde_json::from_slice(bytes).map_err(|_| DecodeFailure::Malformed)?;
    let raw = value.as_object().ok_or(DecodeFailure::Malformed)?;
    if optional_bool(raw, "is_error")?.unwrap_or(false)
        || matches!(
            raw.get("subtype").and_then(Value::as_str),
            Some(
                "error_during_execution"
                    | "error_max_turns"
                    | "error_max_budget_usd"
                    | "error_max_structured_output_retries"
            )
        )
    {
        return Err(classify_reported(raw));
    }
    let mut repairs = Vec::new();
    let structured = raw.get("structured_output").or_else(|| raw.get("structuredOutput"));
    let turn = match structured {
        Some(turn) => model_object(turn, "claude.turn", &mut repairs)?,
        None => direct_turn(raw, &mut repairs)?,
    };
    let mut content = required_string(&turn, "content")?.to_owned();
    let calls = required_calls(&turn)?;
    let mut tool_calls = decode_calls(calls, allowed_tools, max_calls, &mut repairs)?;
    if structured.is_some()
        && tool_calls.is_empty()
        && let Some(embedded) = decode_embedded(&content, allowed_tools, max_calls, &mut repairs)?
    {
        content = embedded.content;
        tool_calls = embedded.tool_calls;
    }
    if content.is_empty() || content.contains('\0') {
        return Err(DecodeFailure::Malformed);
    }
    Ok(RuntimeTurn { content, tool_calls, usage: usage(raw)?, repairs })
}

fn direct_turn(
    raw: &Map<String, Value>,
    repairs: &mut Vec<ModelEvent>,
) -> Result<Map<String, Value>, DecodeFailure> {
    let text = raw.get("result").and_then(Value::as_str).ok_or(DecodeFailure::Incomplete)?;
    if let Some(turn) = completion::public_text(raw, text) {
        return Ok(turn);
    }
    if !text.contains('{') && !text.trim_start().starts_with("```") {
        return Err(DecodeFailure::Incomplete);
    }
    // This field is the model's private transport envelope, not public content. Heal
    // only bounded syntax around one complete object; tool authorization stays strict.
    let (json, audit) = peritus_provider_core::healing::private_envelope(
        text,
        "claude.turn",
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|_| DecodeFailure::Malformed)?
    .into_parts();
    let Value::Object(object) =
        serde_json::from_slice(json.canonical_bytes()).map_err(|_| DecodeFailure::Malformed)?
    else {
        return Err(DecodeFailure::Malformed);
    };
    if object.len() != 2 || !object.contains_key("content") || !object.contains_key("tool_calls") {
        return Err(DecodeFailure::Malformed);
    }
    repairs.extend(audit);
    Ok(object)
}

fn classify_reported(raw: &Map<String, Value>) -> DecodeFailure {
    let mut messages = ["result", "error"]
        .into_iter()
        .filter_map(|field| raw.get(field).and_then(Value::as_str))
        .collect::<Vec<_>>();
    // SDKResultError carries errors: string[], whereas earlier result envelopes
    // carry a single result/error string. None of this private diagnostic text is emitted.
    if let Some(errors) = raw.get("errors") {
        let Some(errors) = errors.as_array() else { return DecodeFailure::Malformed };
        for error in errors {
            let Some(message) = error.as_str() else { return DecodeFailure::Malformed };
            messages.push(message);
        }
    }
    let message = messages.join("\n").to_ascii_lowercase();
    if message.contains("oauth")
        || message.contains("authentication")
        || message.contains("not logged in")
        || message.contains("unauthorized")
    {
        DecodeFailure::Authentication
    } else if message.contains("at capacity")
        || message.contains("model capacity")
        || message.contains("overloaded")
    {
        DecodeFailure::Capacity
    } else if message.contains("context window") || message.contains("context length") {
        DecodeFailure::ContextLimit
    } else if message.contains("invalid model")
        || (message.contains("model")
            && (message.contains("not found")
                || message.contains("does not exist")
                || message.contains("not have access")))
    {
        DecodeFailure::InvalidModel
    } else {
        DecodeFailure::Reported
    }
}

fn required_calls(turn: &Map<String, Value>) -> Result<&[Value], DecodeFailure> {
    turn.get("tool_calls")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(DecodeFailure::Malformed)
}

fn decode_calls(
    calls: &[Value],
    allowed_tools: &BTreeSet<String>,
    max_calls: usize,
    repairs: &mut Vec<ModelEvent>,
) -> Result<Vec<RuntimeToolCall>, DecodeFailure> {
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
        let arguments = model_object(
            call.get("arguments").ok_or(DecodeFailure::Malformed)?,
            &format!("claude.tool_calls[{}].arguments", tool_calls.len()),
            repairs,
        )?;
        tool_calls.push(RuntimeToolCall { name: name.to_owned(), arguments });
    }
    Ok(tool_calls)
}

fn decode_embedded(
    content: &str,
    allowed_tools: &BTreeSet<String>,
    max_calls: usize,
    repairs: &mut Vec<ModelEvent>,
) -> Result<Option<RuntimeTurnContent>, DecodeFailure> {
    // This is public assistant prose, not the typed structured_output field. Preserve the
    // existing exact-JSON embedded-call contract; do not turn fenced examples into tool calls.
    let Ok(value) = CanonicalJson::parse(content, JsonBounds::value(ProtocolLimits::PRODUCTION))
    else {
        return Ok(None);
    };
    let Value::Object(mut object) =
        serde_json::from_slice(value.canonical_bytes()).map_err(|_| DecodeFailure::Malformed)?
    else {
        return Ok(None);
    };
    let Some(calls) = object.remove("tool_calls") else {
        return Ok(None);
    };
    let calls = calls.as_array().ok_or(DecodeFailure::Malformed)?;
    let tool_calls = decode_calls(calls, allowed_tools, max_calls, repairs)?;
    let content = if object.len() == 1 && object.contains_key("content") {
        object.get("content").and_then(Value::as_str).ok_or(DecodeFailure::Malformed)?.to_owned()
    } else {
        Value::Object(object).to_string()
    };
    Ok(Some(RuntimeTurnContent { content, tool_calls }))
}

struct RuntimeTurnContent {
    content: String,
    tool_calls: Vec<RuntimeToolCall>,
}

fn model_object(
    value: &Value,
    target: &str,
    repairs: &mut Vec<ModelEvent>,
) -> Result<Map<String, Value>, DecodeFailure> {
    if let Some(object) = value.as_object() {
        return Ok(object.clone());
    }
    if !value.is_string() {
        return Err(DecodeFailure::Malformed);
    }
    let text = value.to_string();
    let (value, audit) =
        peritus_provider_core::healing::object(&text, target, ProtocolLimits::PRODUCTION)
            .map_err(|_| DecodeFailure::Malformed)?
            .into_parts();
    let Value::Object(object) =
        serde_json::from_slice(value.canonical_bytes()).map_err(|_| DecodeFailure::Malformed)?
    else {
        return Err(DecodeFailure::Malformed);
    };
    repairs.extend(audit);
    Ok(object)
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

#[cfg(test)]
mod tests;
