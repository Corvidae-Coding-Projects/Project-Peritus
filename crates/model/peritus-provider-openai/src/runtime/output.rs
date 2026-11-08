//! Fail-closed decoding of the bounded Codex JSONL turn transcript.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

use peritus_model_protocol::{CanonicalJson, ModelEvent, ProtocolLimits, UsageCounters};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde_json::Value;

mod failure;
mod selection;

use failure::reported_failure;
pub(super) use failure::reported_process_failure;
use selection::select_turn;

const MAX_JSONL_LINES: usize = 100_000;
const MAX_JSONL_LINE_BYTES: usize = 4 * 1024 * 1024;
const MAX_AGENT_MESSAGES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeFailure {
    Authentication,
    Safety,
    RateLimited,
    Capacity,
    QuotaExhausted,
    ModelUnavailable,
    ContextLimit,
    InputLimit,
    Reported,
    Malformed,
    InvalidLifecycle,
    InvalidEnvelope,
    InvalidToolArguments,
    InvalidToolArgumentsAt { call: usize, line: usize, column: usize },
    InvalidToolChoice,
    OutputLimit,
    InvalidUsage,
    MultipleMessages,
    UnsupportedEvent,
    Incomplete,
    NativeTool,
}

pub struct RuntimeTurn {
    pub content: String,
    pub tool_calls: Vec<RuntimeToolCall>,
    pub usage: UsageCounters,
    pub repairs: Vec<ModelEvent>,
}

pub struct RuntimeToolCall {
    pub name: String,
    pub arguments: CanonicalJson,
    pub typed_arguments: bool,
}

struct StructuredTurn {
    content: String,
    tool_calls: Vec<StructuredToolCall>,
}

struct StructuredToolCall {
    name: String,
    arguments_json: String,
    typed_arguments: bool,
}

impl<'de> de::Deserialize<'de> for StructuredTurn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(StructuredTurnVisitor)
    }
}

struct StructuredTurnVisitor;

impl<'de> Visitor<'de> for StructuredTurnVisitor {
    type Value = StructuredTurn;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a content and tool_calls object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut content = None;
        let mut tool_calls = None;
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "content" if content.is_none() => content = Some(map.next_value()?),
                "tool_calls" if tool_calls.is_none() => tool_calls = Some(map.next_value()?),
                "content" | "tool_calls" => return Err(de::Error::duplicate_field("result")),
                _ => return Err(de::Error::unknown_field(&field, &["content", "tool_calls"])),
            }
        }
        Ok(StructuredTurn {
            content: content.ok_or_else(|| de::Error::missing_field("content"))?,
            tool_calls: tool_calls.ok_or_else(|| de::Error::missing_field("tool_calls"))?,
        })
    }
}

impl<'de> de::Deserialize<'de> for StructuredToolCall {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(StructuredToolCallVisitor)
    }
}

struct StructuredToolCallVisitor;

impl<'de> Visitor<'de> for StructuredToolCallVisitor {
    type Value = StructuredToolCall;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("one tool key and arguments, or a historical named tool call")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // A keyed variant makes tool selection precede its payload even when a
        // downstream JSON implementation sorts object properties alphabetically.
        let mut fields = Vec::new();
        while let Some(field) = map.next_key::<String>()? {
            if fields.len() >= 2 || fields.iter().any(|(key, _)| key == &field) {
                return Err(de::Error::custom("duplicate or excessive tool call fields"));
            }
            let raw: Box<serde_json::value::RawValue> = map.next_value()?;
            fields.push((field, raw));
        }
        if fields.len() == 1 {
            let (name, raw) = fields.pop().ok_or_else(|| de::Error::missing_field("tool"))?;
            let typed_arguments = !raw.get().starts_with('"');
            let arguments_json = if typed_arguments {
                raw.get().to_owned()
            } else {
                serde_json::from_str(raw.get()).map_err(de::Error::custom)?
            };
            return Ok(StructuredToolCall { name, arguments_json, typed_arguments });
        }
        // Previously retained named/arguments_json and named/arguments turns
        // remain readable. Mixed forms and duplicate fields never acquire authority.
        let mut name = None;
        let mut arguments_json = None;
        let mut typed_arguments = false;
        for (field, raw) in fields {
            match field.as_str() {
                "name" => name = Some(serde_json::from_str(raw.get()).map_err(de::Error::custom)?),
                "arguments_json" if arguments_json.is_none() => {
                    arguments_json =
                        Some(serde_json::from_str(raw.get()).map_err(de::Error::custom)?);
                }
                "arguments" if arguments_json.is_none() => {
                    arguments_json = Some(raw.get().to_owned());
                    typed_arguments = true;
                }
                _ => {
                    return Err(de::Error::unknown_field(
                        &field,
                        &["name", "arguments_json", "arguments"],
                    ));
                }
            }
        }
        Ok(StructuredToolCall {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            arguments_json: arguments_json
                .ok_or_else(|| de::Error::missing_field("arguments_json"))?,
            typed_arguments,
        })
    }
}

pub struct RuntimeDecoded {
    pub turn: Result<RuntimeTurn, DecodeFailure>,
    pub usage: UsageCounters,
}

pub fn decode_with_usage(
    stdout: &[u8],
    allowed_tools: &BTreeSet<String>,
    call_bounds: std::ops::RangeInclusive<usize>,
    final_message: Option<&str>,
) -> RuntimeDecoded {
    let mut state = State::default();
    let turn = decode_turn(stdout, allowed_tools, call_bounds, final_message, &mut state);
    RuntimeDecoded { turn, usage: state.usage }
}

fn decode_turn(
    stdout: &[u8],
    allowed_tools: &BTreeSet<String>,
    call_bounds: std::ops::RangeInclusive<usize>,
    final_message: Option<&str>,
    state: &mut State,
) -> Result<RuntimeTurn, DecodeFailure> {
    let mut seen = HashSet::new();
    for raw_line in stdout.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        state.raw_events = state.raw_events.checked_add(1).ok_or(DecodeFailure::OutputLimit)?;
        if state.raw_events > MAX_JSONL_LINES || line.len() > MAX_JSONL_LINE_BYTES {
            return Err(DecodeFailure::OutputLimit);
        }
        if !seen.insert(line.to_vec()) {
            state.duplicates = state.duplicates.checked_add(1).ok_or(DecodeFailure::OutputLimit)?;
            continue;
        }
        if state.completed {
            return Err(DecodeFailure::InvalidLifecycle);
        }
        let event: Value = serde_json::from_slice(line).map_err(|_| DecodeFailure::Malformed)?;
        decode_event(&event, state)?;
    }
    if !state.completed {
        return Err(DecodeFailure::Incomplete);
    }
    select_turn(state, final_message, allowed_tools, call_bounds)
}

struct State {
    thread_started: bool,
    turn_started: bool,
    completed: bool,
    assistant_messages: Vec<String>,
    usage: UsageCounters,
    raw_events: usize,
    duplicates: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            thread_started: false,
            turn_started: false,
            completed: false,
            assistant_messages: Vec::new(),
            usage: UsageCounters::new(None, None, None, None, None, None, None, None),
            raw_events: 0,
            duplicates: 0,
        }
    }
}

fn decode_event(event: &Value, state: &mut State) -> Result<(), DecodeFailure> {
    let event_type = string(event, "type")?;
    match event_type {
        "thread.started" => {
            if state.thread_started || string(event, "thread_id")?.len() > 512 {
                return Err(DecodeFailure::InvalidLifecycle);
            }
            state.thread_started = true;
            Ok(())
        }
        "turn.started" => {
            if state.turn_started {
                return Err(DecodeFailure::InvalidLifecycle);
            }
            state.turn_started = true;
            Ok(())
        }
        "item.started" | "item.updated" | "item.completed" => decode_item(event, event_type, state),
        "turn.completed" => {
            state.usage = decode_usage(event.get("usage"))?;
            state.completed = true;
            Ok(())
        }
        "turn.failed" | "error" => {
            if event.get("usage").is_some() {
                state.usage = decode_usage(event.get("usage"))?;
            }
            Err(reported_failure(event))
        }
        _ => Err(DecodeFailure::UnsupportedEvent),
    }
}

fn decode_item(event: &Value, event_type: &str, state: &mut State) -> Result<(), DecodeFailure> {
    let item = event.get("item").and_then(Value::as_object).ok_or(DecodeFailure::Malformed)?;
    let item_type = item.get("type").and_then(Value::as_str).ok_or(DecodeFailure::Malformed)?;
    if native_tool_item(item_type) {
        return Err(DecodeFailure::NativeTool);
    }
    match item_type {
        "reasoning" => Ok(()),
        "error" if event_type == "item.completed" => {
            // Codex ErrorItem is a nonfatal notification (for example missing model
            // metadata). Keep reading for its authoritative error/turn terminal.
            let message =
                item.get("message").and_then(Value::as_str).ok_or(DecodeFailure::Malformed)?;
            if message.len() > ProtocolLimits::PRODUCTION.max_text_bytes() || message.contains('\0')
            {
                return Err(DecodeFailure::OutputLimit);
            }
            Ok(())
        }
        "agent_message" if event_type != "item.completed" => Ok(()),
        "agent_message" => {
            let text = item.get("text").and_then(Value::as_str).ok_or(DecodeFailure::Malformed)?;
            if text.len() > ProtocolLimits::PRODUCTION.max_text_bytes() || text.contains('\0') {
                return Err(DecodeFailure::OutputLimit);
            }
            if state.assistant_messages.len() >= MAX_AGENT_MESSAGES {
                return Err(DecodeFailure::OutputLimit);
            }
            state.assistant_messages.push(text.to_owned());
            Ok(())
        }
        _ => Err(DecodeFailure::UnsupportedEvent),
    }
}

fn validate_turn(
    turn: StructuredTurn,
    allowed_tools: &BTreeSet<String>,
    call_bounds: std::ops::RangeInclusive<usize>,
    state: &State,
    mut repairs: Vec<ModelEvent>,
) -> Result<RuntimeTurn, DecodeFailure> {
    if turn.content.len() > ProtocolLimits::PRODUCTION.max_text_bytes()
        || turn.content.contains('\0')
    {
        return Err(DecodeFailure::OutputLimit);
    }
    if !call_bounds.contains(&turn.tool_calls.len()) {
        return Err(DecodeFailure::InvalidToolChoice);
    }
    if turn.content.is_empty() && turn.tool_calls.is_empty() {
        return Err(DecodeFailure::InvalidEnvelope);
    }
    let mut tool_calls = Vec::with_capacity(turn.tool_calls.len());
    for call in turn.tool_calls {
        if call.name.trim() != call.name
            || call.name.is_empty()
            || !allowed_tools.contains(&call.name)
        {
            return Err(DecodeFailure::InvalidToolChoice);
        }
        let (arguments, audit) = peritus_provider_core::healing::object(
            &call.arguments_json,
            &format!("codex.tool_calls[{}].arguments_json", tool_calls.len()),
            ProtocolLimits::PRODUCTION,
        )
        .map_err(|_| match serde_json::from_str::<Value>(&call.arguments_json) {
            Err(error) => DecodeFailure::InvalidToolArgumentsAt {
                call: tool_calls.len(),
                line: error.line(),
                column: error.column(),
            },
            Ok(_) => DecodeFailure::InvalidToolArguments,
        })?
        .into_parts();
        repairs.extend(audit);
        tool_calls.push(RuntimeToolCall {
            name: call.name,
            arguments,
            typed_arguments: call.typed_arguments,
        });
    }
    Ok(RuntimeTurn { content: turn.content, tool_calls, usage: state.usage, repairs })
}

fn decode_usage(value: Option<&Value>) -> Result<UsageCounters, DecodeFailure> {
    let Some(value) = value else {
        return Ok(UsageCounters::new(None, None, None, None, None, None, None, None));
    };
    let object = value.as_object().ok_or(DecodeFailure::InvalidUsage)?;
    let input = optional_u64(object.get("input_tokens"))?;
    let cached = optional_u64(object.get("cached_input_tokens"))?;
    let output = optional_u64(object.get("output_tokens"))?;
    let total = optional_u64(object.get("total_tokens"))?;
    if matches!((input, output, total), (Some(left), Some(right), Some(sum)) if left.checked_add(right) != Some(sum))
    {
        return Err(DecodeFailure::InvalidUsage);
    }
    Ok(UsageCounters::new(input, cached, None, output, None, None, total, None))
}

fn optional_u64(value: Option<&Value>) -> Result<Option<u64>, DecodeFailure> {
    value.map_or(Ok(None), |value| value.as_u64().map(Some).ok_or(DecodeFailure::InvalidUsage))
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, DecodeFailure> {
    value.get(field).and_then(Value::as_str).ok_or(DecodeFailure::Malformed)
}

fn native_tool_item(item_type: &str) -> bool {
    matches!(
        item_type,
        "command_execution"
            | "file_change"
            | "mcp_tool_call"
            | "dynamic_tool_call"
            | "web_search"
            | "image_generation"
            | "computer_use"
    )
}

#[cfg(test)]
mod contract_replays {
    use super::*;
    use serde_json::json;
    fn transcript(text: &str) -> Vec<u8> {
        [json!({"type":"thread.started","thread_id":"replay"}), json!({"type":"turn.started"}), json!({"type":"item.completed","item":{"type":"agent_message","text":text}}), json!({"type":"turn.completed","usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}})].iter().map(Value::to_string).collect::<Vec<_>>().join("\n").into_bytes()
    }
    #[test]
    fn typed_and_historical_arguments_share_fail_closed_decoder() {
        let allowed = BTreeSet::from(["record".to_owned()]);
        for (field, value) in [
            ("arguments", json!({"label":"exact","nullable":null})),
            ("arguments_json", json!("{\"label\":\"exact\",\"nullable\":null}")),
        ] {
            let text =
                json!({"content":"","tool_calls":[{"name":"record",field:value}]}).to_string();
            let result = decode_with_usage(&transcript(&text), &allowed, 1..=1, None).turn.unwrap();
            assert_eq!(
                result.tool_calls[0].arguments.to_wire_string(),
                "{\"label\":\"exact\",\"nullable\":null}"
            );
            assert_eq!(result.tool_calls[0].typed_arguments, field == "arguments");
        }
        for value in [
            json!({"label":"exact","nullable":null}),
            json!("{\"label\":\"exact\",\"nullable\":null}"),
        ] {
            let typed = value.is_object();
            let text = json!({"content":"","tool_calls":[{"record":value}]}).to_string();
            let result = decode_with_usage(&transcript(&text), &allowed, 1..=1, None).turn.unwrap();
            assert_eq!(result.tool_calls[0].name, "record");
            assert_eq!(result.tool_calls[0].typed_arguments, typed);
            assert_eq!(
                result.tool_calls[0].arguments.to_wire_string(),
                "{\"label\":\"exact\",\"nullable\":null}"
            );
        }
        for text in [
            r#"{"content":"","tool_calls":[{"name":"record","arguments":{},"arguments_json":"{}"}]}"#,
            r#"{"content":"","tool_calls":[{"name":"record","arguments":{"label":"first","label":"last"}}]}"#,
        ] {
            assert!(decode_with_usage(&transcript(text), &allowed, 1..=1, None).turn.is_err());
        }
    }
}
