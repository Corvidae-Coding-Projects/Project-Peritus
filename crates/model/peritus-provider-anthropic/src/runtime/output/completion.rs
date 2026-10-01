//! Explicit native completion evidence for tool-free public text, never private tool requests.

use serde_json::{Map, Value};

pub(super) fn public_text(raw: &Map<String, Value>, text: &str) -> Option<Map<String, Value>> {
    // The official SDK's success result is text. Without native structured-output mode,
    // it may be public prose rather than the requested private host-tool envelope.
    // Require a completed single turn and no denied/deferred/background activity;
    // incomplete legacy result strings and all private-shaped candidates remain strict.
    let completed_fields = [
        ("type", "result"),
        ("subtype", "success"),
        ("stop_reason", "end_turn"),
        ("terminal_reason", "completed"),
    ];
    if !completed_fields
        .iter()
        .all(|(field, expected)| raw.get(*field).and_then(Value::as_str) == Some(*expected))
        || raw.get("is_error").and_then(Value::as_bool) != Some(false)
        || raw.get("num_turns").and_then(Value::as_u64) != Some(1)
        || !no_side_activity(raw)
        || text.trim().is_empty()
        || private_candidate(text)
    {
        return None;
    }
    Some(Map::from_iter([
        ("content".to_owned(), Value::String(text.to_owned())),
        ("tool_calls".to_owned(), Value::Array(Vec::new())),
    ]))
}

fn no_side_activity(raw: &Map<String, Value>) -> bool {
    raw.get("permission_denials").and_then(Value::as_array).is_some_and(Vec::is_empty)
        && raw.get("errors").is_none_or(|errors| errors.as_array().is_some_and(Vec::is_empty))
        && ["api_error_status", "deferred_tool_use"]
            .iter()
            .all(|field| raw.get(*field).is_none_or(Value::is_null))
        && !raw.contains_key("local_command")
        && raw
            .get("origin")
            .is_none_or(|origin| origin.get("kind").and_then(Value::as_str) == Some("human"))
        && raw.get("queued_turn_count").is_none_or(|queued| queued.as_u64() == Some(0))
        && raw
            .get("subagent_stats")
            .is_none_or(|stats| stats.get("spawned").and_then(Value::as_u64) == Some(0))
}

fn private_candidate(text: &str) -> bool {
    // A completed native Markdown document is public content even when it includes
    // code, JSON, or tool examples. Its heading distinguishes it from the private
    // object grammar; never extract operations from anywhere inside the document.
    let first = text.trim_start();
    let heading = first.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&heading)
        && first.as_bytes().get(heading).is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        return false;
    }
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with(['{', '[']) || line.starts_with("```")
    }) || text.contains("tool_calls")
        || (text.contains('{') && text.contains("content"))
        || text.find(['{', '[']).is_some_and(|start| text[..start].trim_end().ends_with(':'))
}
