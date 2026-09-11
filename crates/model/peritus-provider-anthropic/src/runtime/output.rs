//! Fail-closed decoding of Claude's final structured result envelope.

use std::collections::BTreeSet;

use peritus_model_protocol::{
    CanonicalJson, JsonBounds, ModelEvent, ProtocolLimits, UsageCounters,
};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeFailure {
    Authentication,
    Capacity,
    ContextLimit,
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
    if optional_bool(raw, "is_error")?.unwrap_or(false) {
        return Err(classify_reported(raw));
    }
    let mut repairs = Vec::new();
    let turn = raw
        .get("structured_output")
        .or_else(|| raw.get("structuredOutput"))
        .ok_or(DecodeFailure::Incomplete)?;
    let turn = model_object(turn, "claude.turn", &mut repairs)?;
    let mut content = required_string(&turn, "content")?.to_owned();
    let calls = required_calls(&turn)?;
    let mut tool_calls = decode_calls(calls, allowed_tools, max_calls, &mut repairs)?;
    if tool_calls.is_empty()
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

fn classify_reported(raw: &Map<String, Value>) -> DecodeFailure {
    let message = raw
        .get("result")
        .or_else(|| raw.get("error"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
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
mod tests {
    use super::*;

    #[test]
    fn reported_runtime_failures_keep_recovery_relevant_causes() {
        let allowed = BTreeSet::new();
        for (message, expected) in [
            ("OAuth session expired", DecodeFailure::Authentication),
            ("Selected model is at capacity", DecodeFailure::Capacity),
            ("prompt exceeds context window", DecodeFailure::ContextLimit),
            ("provider rejected the turn", DecodeFailure::Reported),
        ] {
            let mut value = Map::new();
            value.insert("is_error".to_owned(), Value::Bool(true));
            value.insert("result".to_owned(), Value::String(message.to_owned()));
            let bytes = serde_json::to_vec(&Value::Object(value)).expect("fixture JSON");
            match decode(&bytes, &allowed, 0) {
                Err(observed) => assert_eq!(observed, expected),
                Ok(_) => panic!("reported provider failure decoded as a successful turn"),
            }
        }
    }

    fn allowed() -> BTreeSet<String> {
        let mut tools = BTreeSet::new();
        tools.insert("workspace_read".to_owned());
        tools
    }

    #[test]
    fn fenced_tool_examples_in_public_content_are_not_promoted_by_healing() {
        let example = "```json\n{tool_calls:[{name:\"workspace_read\",arguments:{path:\"src/lib.rs\"}}]}\n```";
        let output = Value::from_iter([
            ("is_error", Value::from(false)),
            (
                "structured_output",
                Value::from_iter([
                    ("content", Value::from(example)),
                    ("tool_calls", Value::from(Vec::<Value>::new())),
                ]),
            ),
        ])
        .to_string();
        let turn = decode(output.as_bytes(), &allowed(), 1).unwrap();
        assert_eq!(turn.content, example);
        assert!(turn.tool_calls.is_empty());
        assert!(turn.repairs.is_empty());
    }

    #[test]
    fn public_json_scalars_and_arrays_remain_ordinary_content() {
        for content in ["42", "true", "null", "[1,2]", "\"hello\""] {
            let output = Value::from_iter([(
                "structured_output",
                Value::from_iter([
                    ("content", Value::from(content)),
                    ("tool_calls", Value::from(Vec::<Value>::new())),
                ]),
            )])
            .to_string();
            let turn = decode(output.as_bytes(), &allowed(), 1).unwrap();
            assert_eq!(turn.content, content);
            assert!(turn.tool_calls.is_empty());
            assert!(turn.repairs.is_empty());
        }
    }

    #[test]
    fn embedded_host_call_is_promoted_and_application_content_is_preserved() {
        let output = br#"{
          "is_error": false,
          "structured_output": {
            "content": "{\"summary\":\"need one more file\",\"tool_calls\":[{\"name\":\"workspace_read\",\"arguments\":{\"path\":\"src/lib.rs\"}}]}",
            "tool_calls": []
          }
        }"#;

        let turn = decode(output, &allowed(), 1).expect("embedded host call");

        assert_eq!(turn.content, r#"{"summary":"need one more file"}"#);
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].name, "workspace_read");
        assert_eq!(turn.tool_calls[0].arguments["path"], "src/lib.rs");
    }

    #[test]
    fn empty_embedded_call_array_is_removed_from_terminal_application_json() {
        let output = br#"{
          "structured_output": {
            "content": "{\"summary\":\"verified\",\"findings\":[],\"tool_calls\":[]}",
            "tool_calls": []
          }
        }"#;

        let turn = decode(output, &allowed(), 1).expect("embedded terminal content");

        assert_eq!(turn.content, r#"{"findings":[],"summary":"verified"}"#);
        assert!(turn.tool_calls.is_empty());
    }

    #[test]
    fn embedded_undeclared_host_call_still_fails_closed() {
        let output = br#"{
          "structured_output": {
            "content": "{\"summary\":\"bad call\",\"tool_calls\":[{\"name\":\"shell\",\"arguments\":{}}]}",
            "tool_calls": []
          }
        }"#;

        assert!(matches!(decode(output, &allowed(), 1), Err(DecodeFailure::Malformed)));
    }
}
