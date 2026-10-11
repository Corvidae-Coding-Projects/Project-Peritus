//! Model-visible bounds for exact tool observations retained in the durable trace.

use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits};
use serde_json::{Map, Value};

use super::{DeveloperLoopError, context::digest_hex};

const POLICY: &str = "peritus-tool-output-v1";
const COMMAND_POLICY: &str = "peritus-command-output-v1";
const MAX_TOKENS: u64 = 10_000;
const CONTEXT_FRACTION: u64 = 8;
const MIN_TOKENS: u64 = 512;
const BYTES_PER_TOKEN: usize = 3;
const RETAINED_ACTIVE_PROGRESS_EVENTS: usize = 8;

/// Retains exact output in the caller-owned trace while bounding the copy admitted to model history.
pub(super) fn model_visible_tool_output(
    tool_name: &str,
    output: &CanonicalJson,
    provider_input_tokens: u64,
    limits: ProtocolLimits,
) -> Result<CanonicalJson, DeveloperLoopError> {
    let output = project_command_output(tool_name, output, limits)?;
    let token_budget = (provider_input_tokens / CONTEXT_FRACTION).clamp(MIN_TOKENS, MAX_TOKENS);
    let byte_budget = usize::try_from(token_budget)
        .unwrap_or(usize::MAX)
        .saturating_mul(BYTES_PER_TOKEN)
        .min(JsonBounds::value(limits).max_bytes());
    let exact = output.canonical_bytes();
    if exact.len() <= byte_budget {
        return Ok(output);
    }

    let text = output.to_wire_string();
    let original_digest = digest_hex(output.digest());
    let original_token_estimate = exact.len().div_ceil(BYTES_PER_TOKEN);
    let mut preview_chars = byte_budget / 8;
    loop {
        let (head, tail) = head_tail(&text, preview_chars);
        let mut details = Map::new();
        details.insert("head".to_owned(), Value::String(head));
        details.insert("original_bytes".to_owned(), Value::from(exact.len()));
        details.insert("original_sha256".to_owned(), Value::String(original_digest.clone()));
        details.insert("original_token_estimate".to_owned(), Value::from(original_token_estimate));
        details.insert("policy".to_owned(), Value::String(POLICY.to_owned()));
        details.insert("tail".to_owned(), Value::String(tail));
        details.insert(
            "warning".to_owned(),
            Value::String(
                "Tool output was truncated before model context. The exact output remains in the run trace; use a narrower tool request for omitted detail."
                    .to_owned(),
            ),
        );
        let mut root = Map::new();
        root.insert("peritus_truncated_tool_output".to_owned(), Value::Object(details));
        let value = Value::Object(root);
        let rendered = serde_json::to_string(&value).map_err(|_| {
            DeveloperLoopError::Context("tool observation truncation could not encode JSON".into())
        })?;
        if rendered.len() <= byte_budget {
            return Ok(CanonicalJson::parse(&rendered, JsonBounds::value(limits))?);
        }
        if preview_chars == 0 {
            return Err(DeveloperLoopError::Context(
                "tool observation truncation metadata exceeded its model-visible budget".into(),
            ));
        }
        preview_chars /= 2;
    }
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
            | "command_stdin"
            | "command_resize"
            | "command_signal"
            | "command_cancel"
            | "command_recover"
    )
}

fn head_tail(value: &str, chars: usize) -> (String, String) {
    let head = value.chars().take(chars).collect();
    let mut tail = value.chars().rev().take(chars).collect::<Vec<_>>();
    tail.reverse();
    (head, tail.into_iter().collect())
}
