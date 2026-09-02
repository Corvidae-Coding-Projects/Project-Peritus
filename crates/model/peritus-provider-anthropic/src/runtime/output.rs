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
    let mut content = required_string(turn, "content")?.to_owned();
    let calls = required_calls(turn)?;
    let mut tool_calls = decode_calls(calls, allowed_tools, max_calls)?;
    if tool_calls.is_empty()
        && let Some(embedded) = decode_embedded(&content, allowed_tools, max_calls)?
    {
        content = embedded.content;
        tool_calls = embedded.tool_calls;
    }
    if content.is_empty() || content.contains('\0') {
        return Err(DecodeFailure::Malformed);
    }
    Ok(RuntimeTurn { content, tool_calls, usage: usage(raw)? })
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
        let arguments = call
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or(DecodeFailure::Malformed)?
            .clone();
        tool_calls.push(RuntimeToolCall { name: name.to_owned(), arguments });
    }
    Ok(tool_calls)
}

fn decode_embedded(
    content: &str,
    allowed_tools: &BTreeSet<String>,
    max_calls: usize,
) -> Result<Option<RuntimeTurnContent>, DecodeFailure> {
    let Ok(Value::Object(mut object)) = serde_json::from_str(content) else {
        return Ok(None);
    };
    let Some(calls) = object.remove("tool_calls") else {
        return Ok(None);
    };
    let calls = calls.as_array().ok_or(DecodeFailure::Malformed)?;
    let tool_calls = decode_calls(calls, allowed_tools, max_calls)?;
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

    fn allowed() -> BTreeSet<String> {
        let mut tools = BTreeSet::new();
        tools.insert("workspace_read".to_owned());
        tools
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
