//! Normalized response, tool, and usage reconstruction from D0 trace frames.

use std::{collections::BTreeMap, path::Path};

use peritus_model_protocol::{ModelEvent, ProtocolLimits, UsageCounters, decode_event_envelope};
use serde_json::{Value, json};

use super::{bounded, frames::Frame};
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
    calls: Vec<ActiveToolCall>,
    call_indexes: BTreeMap<String, usize>,
    model: String,
    usage: UsageCounters,
    observed_cache_tokens: Option<u64>,
}

struct ActiveToolCall {
    id: String,
    name: String,
    argument_bytes: Vec<u8>,
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
            response.calls.push(ActiveToolCall {
                id,
                name: name.as_str().to_owned(),
                argument_bytes: Vec::new(),
            });
        }
        ModelEvent::ToolArgumentDelta { call_id, fragment } => {
            let response = current(path, active)?;
            let index =
                response.call_indexes.get(call_id.expose_for_wire()).copied().ok_or_else(|| {
                    BenchmarkError::trace(path, "tool arguments precede their call")
                })?;
            response.calls[index].argument_bytes.extend_from_slice(fragment.expose());
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
    let assistant_text = bounded::assistant(&assistant_text);
    let tool_calls = finalize_calls(path, response.calls)?;
    history.push(json!({"role": "assistant", "content": assistant_text}));
    rounds.push(Round {
        request_messages: response.request_messages,
        assistant_text,
        tool_calls,
        model: response.model,
        usage: response.usage,
        observed_cache_tokens: response.observed_cache_tokens,
    });
    Ok(())
}

fn finalize_calls(
    path: &Path,
    calls: Vec<ActiveToolCall>,
) -> Result<Vec<ToolCall>, BenchmarkError> {
    calls
        .into_iter()
        .map(|call| {
            let arguments = String::from_utf8(call.argument_bytes).map_err(|_| {
                BenchmarkError::trace(path, "complete tool arguments are not UTF-8")
            })?;
            let arguments = bounded::tool_arguments(&arguments);
            Ok(ToolCall { id: call.id, name: call.name, arguments })
        })
        .collect()
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
    history.push(json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": bounded::tool_output(output),
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_arguments_decode_after_split_utf8_fragments_are_reassembled() {
        let bytes = "{\"subject\":\"café\"}".as_bytes();
        let split = bytes.iter().position(|byte| *byte == 0xC3).expect("multibyte character") + 1;
        let mut call = ActiveToolCall {
            id: "call-1".to_owned(),
            name: "workspace_write".to_owned(),
            argument_bytes: Vec::new(),
        };
        call.argument_bytes.extend_from_slice(&bytes[..split]);
        assert!(std::str::from_utf8(&call.argument_bytes).is_err());
        call.argument_bytes.extend_from_slice(&bytes[split..]);

        let calls = finalize_calls(Path::new("trace"), vec![call]).expect("complete UTF-8");

        assert_eq!(calls[0].arguments, "{\"subject\":\"café\"}");
    }
}
