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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_command_projection_keeps_decision_evidence_and_drops_lifecycle_noise() {
        let output = json(
            r#"{"failure":null,"handle":"owned-command","progress":[{"kind":"running","message":"started","sequence":1},{"kind":"completed","message":"exited","sequence":2}],"state":"completed","status":"succeeded","stderr":"","stdout":"reviewed branch\n","success":true,"tool_result":{"cleanup":"complete","resources":{"memory":42}}}"#,
        );

        let projected =
            model_visible_tool_output("run_command", &output, 32_768, ProtocolLimits::PRODUCTION)
                .expect("projection");
        let value: Value = serde_json::from_slice(projected.canonical_bytes()).expect("JSON");

        assert_eq!(value["success"], true);
        assert_eq!(value["status"], "succeeded");
        assert_eq!(value["stdout"], "reviewed branch\n");
        assert_eq!(value["handle"], "owned-command");
        assert!(value.get("progress").is_none());
        assert!(value.get("tool_result").is_none());
        assert_eq!(value["peritus_model_projection"]["policy"], COMMAND_POLICY);
        assert_eq!(
            value["peritus_model_projection"]["exact_output_sha256"].as_str().map(str::len),
            Some(64)
        );
    }

    #[test]
    fn running_command_projection_retains_recent_progress_and_handle() {
        let events = (0..12)
            .map(|sequence| {
                format!(
                    r#"{{"kind":"running","message":"event-{sequence}","sequence":{sequence}}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let output = json(&format!(
            r#"{{"handle":"owned-command","progress":[{events}],"state":"running","success":true}}"#
        ));

        let projected =
            model_visible_tool_output("command_poll", &output, 32_768, ProtocolLimits::PRODUCTION)
                .expect("projection");
        let value: Value = serde_json::from_slice(projected.canonical_bytes()).expect("JSON");

        assert_eq!(value["handle"], "owned-command");
        assert_eq!(value["progress"].as_array().map(Vec::len), Some(8));
        assert_eq!(value["progress"][0]["message"], "event-4");
        assert_eq!(value["progress_events_omitted"], 4);
    }

    #[test]
    fn non_command_output_is_unchanged() {
        let output = json(r#"{"entries":[{"path":"src/lib.rs"}]}"#);

        let projected = model_visible_tool_output(
            "workspace_list",
            &output,
            32_768,
            ProtocolLimits::PRODUCTION,
        )
        .expect("unchanged output");

        assert_eq!(projected, output);
    }

    fn json(value: &str) -> CanonicalJson {
        CanonicalJson::parse(value, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .expect("canonical JSON")
    }
}
