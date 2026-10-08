//! Fail-closed decoding of the bounded Codex JSONL turn transcript.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

use peritus_model_protocol::{CanonicalJson, ModelEvent, ProtocolLimits, UsageCounters};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde_json::Value;

mod failure;
mod selection;
pub(super) mod usage;

use failure::reported_failure;
use selection::select_turn;
use usage::decode as decode_usage;

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
    ContextLimit,
    Reported,
    Malformed,
    InvalidLifecycle,
    InvalidEnvelope,
    InvalidToolArguments,
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
    #[cfg(test)]
    pub raw_events: usize,
    #[cfg(test)]
    pub duplicates: usize,
}

pub struct RuntimeToolCall {
    pub name: String,
    pub arguments: CanonicalJson,
}

struct StructuredTurn {
    content: String,
    tool_calls: Vec<StructuredToolCall>,
}

struct StructuredToolCall {
    name: String,
    arguments_json: String,
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
        formatter.write_str("a name and arguments_json object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut name = None;
        let mut arguments_json = None;
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "name" if name.is_none() => name = Some(map.next_value()?),
                "arguments_json" if arguments_json.is_none() => {
                    arguments_json = Some(map.next_value()?);
                }
                "name" | "arguments_json" => {
                    return Err(de::Error::duplicate_field("tool call"));
                }
                _ => {
                    return Err(de::Error::unknown_field(&field, &["name", "arguments_json"]));
                }
            }
        }
        Ok(StructuredToolCall {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            arguments_json: arguments_json
                .ok_or_else(|| de::Error::missing_field("arguments_json"))?,
        })
    }
}

pub struct RuntimeDecoded {
    pub turn: Result<RuntimeTurn, DecodeFailure>,
    pub usage: UsageCounters,
}

impl RuntimeDecoded {
    pub fn relative_to(mut self, previous: UsageCounters) -> Self {
        match usage::relative(self.usage, previous) {
            Ok(usage) => {
                self.usage = usage;
                if let Ok(turn) = &mut self.turn {
                    turn.usage = usage;
                }
            }
            Err(error) => {
                self.turn = Err(error);
                self.usage = UsageCounters::default();
            }
        }
        self
    }
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

#[cfg(test)]
pub fn decode(
    stdout: &[u8],
    allowed_tools: &BTreeSet<String>,
    call_bounds: std::ops::RangeInclusive<usize>,
    final_message: Option<&str>,
) -> Result<RuntimeTurn, DecodeFailure> {
    decode_with_usage(stdout, allowed_tools, call_bounds, final_message).turn
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
        "turn.failed" | "error" => Err(reported_failure(event)),
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
        .map_err(|_| DecodeFailure::InvalidToolArguments)?
        .into_parts();
        repairs.extend(audit);
        tool_calls.push(RuntimeToolCall { name: call.name, arguments });
    }
    Ok(RuntimeTurn {
        content: turn.content,
        tool_calls,
        usage: state.usage,
        repairs,
        #[cfg(test)]
        raw_events: state.raw_events,
        #[cfg(test)]
        duplicates: state.duplicates,
    })
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
