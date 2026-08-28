//! Normalized response, tool, and usage reconstruction from D0 trace frames.

use std::{collections::BTreeMap, path::Path};

use peritus_model_protocol::{ModelEvent, ProtocolLimits, UsageCounters, decode_event_envelope};
use serde_json::{Value, json};

use super::frames::Frame;
use crate::BenchmarkError;

pub(super) struct Round {
    pub request_messages: Vec<Value>,
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub usage: UsageCounters,
    pub observed_cache_tokens: Option<u64>,
}

pub(super) struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

struct ActiveResponse {
    request_messages: Vec<Value>,
    assistant_bytes: Vec<u8>,
    calls: Vec<ToolCall>,
    call_indexes: BTreeMap<String, usize>,
    model: String,
    usage: UsageCounters,
    observed_cache_tokens: Option<u64>,
}

pub(super) fn project(
    path: &Path,
    frames: &[Frame],
    initial_user_prompt: &str,
) -> Result<Vec<Round>, BenchmarkError> {
    let mut history = vec![json!({"role": "user", "content": initial_user_prompt})];
    let mut rounds = Vec::new();
    let mut active: Option<ActiveResponse> = None;
    for frame in frames {
        match frame.tag {
            1 => {
                let envelope = decode_event_envelope(&frame.payload, ProtocolLimits::PRODUCTION)
                    .map_err(|error| BenchmarkError::trace(path, error.to_string()))?;
                apply_event(path, envelope.event(), &mut history, &mut rounds, &mut active)?;
            }
            2 => apply_observation(path, &frame.payload, &mut history)?,
            _ => return Err(BenchmarkError::trace(path, "trace frame tag was not validated")),
        }
    }
    if active.is_some() {
        return Err(BenchmarkError::trace(path, "provider response has no terminal event"));
    }
    Ok(rounds)
}

fn apply_event(
    path: &Path,
    event: &ModelEvent,
    history: &mut Vec<Value>,
    rounds: &mut Vec<Round>,
    active: &mut Option<ActiveResponse>,
) -> Result<(), BenchmarkError> {
    match event {
        ModelEvent::ResponseStarted { model, .. } => {
            if active.is_some() {
                return Err(BenchmarkError::trace(path, "provider responses overlap"));
            }
            *active = Some(ActiveResponse {
                request_messages: history.clone(),
                assistant_bytes: Vec::new(),
                calls: Vec::new(),
                call_indexes: BTreeMap::new(),
                model: model.as_ref().map_or("unknown", |value| value.as_str()).to_owned(),
                usage: UsageCounters::default(),
                observed_cache_tokens: None,
            });
        }
        ModelEvent::TextDelta { fragment, .. } => {
            current(path, active)?.assistant_bytes.extend_from_slice(fragment.expose());
        }
        ModelEvent::ToolCallStarted { call_id, name, .. } => {
            let response = current(path, active)?;
            let id = call_id.expose_for_wire().to_owned();
            if response.call_indexes.contains_key(&id) {
                return Err(BenchmarkError::trace(path, "tool call identity was repeated"));
            }
            response.call_indexes.insert(id.clone(), response.calls.len());
            response.calls.push(ToolCall {
                id,
                name: name.as_str().to_owned(),
                arguments: String::new(),
            });
        }
        ModelEvent::ToolArgumentDelta { call_id, fragment } => {
            let response = current(path, active)?;
            let index =
                response.call_indexes.get(call_id.expose_for_wire()).copied().ok_or_else(|| {
                    BenchmarkError::trace(path, "tool arguments precede their call")
                })?;
            let value = std::str::from_utf8(fragment.expose())
                .map_err(|_| BenchmarkError::trace(path, "tool arguments are not UTF-8"))?;
            response.calls[index].arguments.push_str(value);
        }
        ModelEvent::Usage(observation) => current(path, active)?.usage = observation.counters(),
        ModelEvent::Cache(observation) => {
            let response = current(path, active)?;
            response.observed_cache_tokens =
                observation.input_tokens().or(response.observed_cache_tokens);
        }
        ModelEvent::ResponseCompleted => complete(path, history, rounds, active)?,
        ModelEvent::ResponseFailed(_) | ModelEvent::ResponseCancelled => {
            *active = None;
        }
        _ => {}
    }
    Ok(())
}

fn current<'a>(
    path: &Path,
    active: &'a mut Option<ActiveResponse>,
) -> Result<&'a mut ActiveResponse, BenchmarkError> {
    active
        .as_mut()
        .ok_or_else(|| BenchmarkError::trace(path, "provider event has no active response"))
}

fn complete(
    path: &Path,
    history: &mut Vec<Value>,
    rounds: &mut Vec<Round>,
    active: &mut Option<ActiveResponse>,
) -> Result<(), BenchmarkError> {
    let response = active
        .take()
        .ok_or_else(|| BenchmarkError::trace(path, "provider terminal has no active response"))?;
    let assistant_text = String::from_utf8(response.assistant_bytes)
        .map_err(|_| BenchmarkError::trace(path, "assistant response is not UTF-8"))?;
    history.push(json!({"role": "assistant", "content": assistant_text}));
    rounds.push(Round {
        request_messages: response.request_messages,
        assistant_text,
        tool_calls: response.calls,
        model: response.model,
        usage: response.usage,
        observed_cache_tokens: response.observed_cache_tokens,
    });
    Ok(())
}

fn apply_observation(
    path: &Path,
    payload: &[u8],
    history: &mut Vec<Value>,
) -> Result<(), BenchmarkError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| BenchmarkError::trace(path, error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| BenchmarkError::trace(path, "tool observation is not an object"))?;
    let call_id = object
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or_else(|| BenchmarkError::trace(path, "tool observation has no call identity"))?;
    let output = object
        .get("output")
        .and_then(Value::as_str)
        .ok_or_else(|| BenchmarkError::trace(path, "tool observation has no output"))?;
    history.push(json!({"role": "tool", "tool_call_id": call_id, "content": output}));
    Ok(())
}
