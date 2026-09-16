//! Normalized response, tool, and usage reconstruction from D0 trace frames.

use std::{collections::BTreeMap, path::Path};

use peritus_model_protocol::{ModelEvent, ProtocolLimits, UsageCounters, decode_event_envelope};
use peritus_product_runner::DeveloperTraceFrameKind;
use serde_json::{Value, json};

use super::{bounded, frames::Frame, metadata};
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

pub(super) struct ProjectedTrace {
    pub rounds: Vec<Round>,
    pub incomplete_response: bool,
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
) -> Result<ProjectedTrace, BenchmarkError> {
    let mut history = vec![json!({"role": "user", "content": initial_user_prompt})];
    let mut rounds = Vec::new();
    let mut active: Option<ActiveResponse> = None;
    for frame in frames {
        match frame.kind {
            DeveloperTraceFrameKind::ProviderEnvelope => {
                let envelope = decode_event_envelope(&frame.payload, ProtocolLimits::PRODUCTION)
                    .map_err(|error| BenchmarkError::trace(path, error.to_string()))?;
                apply_event(path, envelope.event(), &mut history, &mut rounds, &mut active)?;
            }
            DeveloperTraceFrameKind::ToolObservation => {
                apply_observation(path, &frame.payload, &mut history)?;
            }
            DeveloperTraceFrameKind::LocalMemoryObservation => {
                metadata::validate(path, frame.kind, &frame.payload)?;
                apply_observation(path, &frame.payload, &mut history)?;
            }
            DeveloperTraceFrameKind::ContextCompaction
            | DeveloperTraceFrameKind::LocalMemoryCheckpoint => {
                metadata::validate(path, frame.kind, &frame.payload)?;
            }
            DeveloperTraceFrameKind::RetryScheduled | DeveloperTraceFrameKind::ProviderSwitch => {
                metadata::validate(path, frame.kind, &frame.payload)?;
                active = None;
            }
        }
    }
    Ok(ProjectedTrace { rounds, incomplete_response: active.is_some() })
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
    use peritus_model_protocol::{EventEnvelope, encode_event_envelope};
    use peritus_types::Sha256Digest;

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

    #[test]
    fn local_memory_frames_remain_projectable_benchmark_evidence() {
        let checkpoint = serde_json::to_vec(&json!({
            "schema_version": 2,
            "scope": vec![1_u8; 32],
            "generation": 1,
            "manifest_sha256": vec![2_u8; 32],
            "manifest_bytes": 64,
            "view_sha256": vec![3_u8; 32],
            "state_revision": 1,
            "estimated_input_tokens": 12,
            "validation": {"state_revision": 1}
        }))
        .expect("checkpoint JSON");
        let observation = serde_json::to_vec(&json!({
            "schema_version": 1,
            "scope": vec![1_u8; 32],
            "invocation": 1,
            "tool_sequence": 1,
            "call_id": "call-1",
            "name": "workspace_read",
            "arguments": "{\"path\":\"in/input.txt\"}",
            "output": "{\"text\":\"value\"}",
            "is_error": false
        }))
        .expect("observation JSON");
        let frames = [
            Frame { kind: DeveloperTraceFrameKind::LocalMemoryCheckpoint, payload: checkpoint },
            Frame { kind: DeveloperTraceFrameKind::LocalMemoryObservation, payload: observation },
        ];

        let projected = project(Path::new("trace"), &frames, "task").expect("project trace");

        assert!(projected.rounds.is_empty());
        assert!(!projected.incomplete_response);
    }

    #[test]
    fn completed_rounds_survive_a_trailing_incomplete_response() {
        let frames = [
            event_frame(1, ModelEvent::ResponseStarted { response_id: None, model: None }),
            event_frame(2, ModelEvent::ResponseCompleted),
            event_frame(3, ModelEvent::ResponseStarted { response_id: None, model: None }),
        ];

        let projected = project(Path::new("trace"), &frames, "task").expect("project trace");

        assert_eq!(projected.rounds.len(), 1);
        assert!(projected.incomplete_response);
    }

    fn event_frame(sequence: u64, event: ModelEvent) -> Frame {
        let digest = u8::try_from(sequence).expect("test sequence");
        let envelope =
            EventEnvelope::new(sequence, None, None, Sha256Digest::new([digest; 32]), event)
                .expect("event envelope");
        Frame {
            kind: DeveloperTraceFrameKind::ProviderEnvelope,
            payload: encode_event_envelope(&envelope, ProtocolLimits::PRODUCTION)
                .expect("event encoding"),
        }
    }
}
