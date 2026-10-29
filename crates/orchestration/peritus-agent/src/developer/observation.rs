//! Model-visible bounds for exact tool observations retained in the durable trace.

use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits};
use serde_json::{Map, Value};

use super::{DeveloperLoopError, context::digest_hex};

const COMMAND_POLICY: &str = "peritus-command-output-v1";
const RETAINED_ACTIVE_PROGRESS_EVENTS: usize = 8;

/// Keeps tool content intact; provider-specific transport fitting happens after archival.
/// Duplicate command receipts and obsolete progress events are projected separately.
pub(super) fn model_visible_tool_output(
    tool_name: &str,
    output: &CanonicalJson,
    limits: ProtocolLimits,
) -> Result<CanonicalJson, DeveloperLoopError> {
    project_command_output(tool_name, output, limits)
}

fn project_command_output(
    tool_name: &str,
    output: &CanonicalJson,
    limits: ProtocolLimits,
) -> Result<CanonicalJson, DeveloperLoopError> {
    if !is_command_tool(tool_name) {
        return Ok(output.clone());
    }
    let mut value: Value = serde_json::from_slice(output.canonical_bytes()).map_err(|_| {
        DeveloperLoopError::Context("canonical tool observation could not be decoded".into())
    })?;
    let Some(result) = value.as_object_mut() else {
        return Ok(output.clone());
    };
    let mut omitted = Vec::new();
    if result.remove("tool_result").is_some() {
        omitted.push(Value::String("tool_result".to_owned()));
    }
    if let Some(progress) = result.remove("progress") {
        let completed = result.get("state").and_then(Value::as_str) == Some("completed");
        if completed {
            omitted.push(Value::String("progress".to_owned()));
        } else if let Value::Array(events) = progress {
            let omitted_events = events.len().saturating_sub(RETAINED_ACTIVE_PROGRESS_EVENTS);
            let retained = events
                .into_iter()
                .rev()
                .take(RETAINED_ACTIVE_PROGRESS_EVENTS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            result.insert("progress".to_owned(), Value::Array(retained));
            if omitted_events > 0 {
                result.insert("progress_events_omitted".to_owned(), Value::from(omitted_events));
                omitted.push(Value::String("progress_prefix".to_owned()));
            }
        } else {
            result.insert("progress".to_owned(), progress);
        }
    }
    if omitted.is_empty() {
        return Ok(output.clone());
    }
    let mut metadata = Map::new();
    metadata.insert("exact_output_sha256".to_owned(), Value::String(digest_hex(output.digest())));
    metadata.insert("omitted".to_owned(), Value::Array(omitted));
    metadata.insert("policy".to_owned(), Value::String(COMMAND_POLICY.to_owned()));
    result.insert("peritus_model_projection".to_owned(), Value::Object(metadata));
    let rendered = serde_json::to_string(&value).map_err(|_| {
        DeveloperLoopError::Context("command observation projection could not encode JSON".into())
    })?;
    Ok(CanonicalJson::parse(&rendered, JsonBounds::value(limits))?)
}

fn is_command_tool(name: &str) -> bool {
    matches!(
        name,
        "run_command"
            | "command_start"
            | "command_poll"
            | "command_wait"
            | "command_stdin"
            | "command_resize"
            | "command_signal"
            | "command_cancel"
            | "command_recover"
    )
}
