//! Reviewed hosted Chat Completions request projections and bounded reasoning replay.

use crate::error;
use peritus_model_protocol::{ReasoningReplay, ToolChoice};
use peritus_provider_core::{ProviderCoreError, hosted::HostedService};
use serde_json::{Map, Value};

pub(super) fn request_fields(
    service: HostedService,
    wire: &mut Map<String, Value>,
    choice: &ToolChoice,
) -> Result<(), ProviderCoreError> {
    tool_choice(wire, choice)?;
    if service != HostedService::Groq
        && let Some(limit) = wire.remove("max_completion_tokens")
    {
        wire.insert("max_tokens".to_owned(), limit);
    }
    if wire.get("response_format").and_then(|v| v.get("type")).and_then(Value::as_str)
        == Some("text")
    {
        wire.remove("response_format");
    }
    if matches!(service, HostedService::Fireworks | HostedService::Together) {
        wire.insert(
            "context_length_exceeded_behavior".to_owned(),
            Value::String("error".to_owned()),
        );
    }
    if !wire.contains_key("tools") {
        wire.remove("parallel_tool_calls");
    }
    if let Some(tools) = wire.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if let Some(function) = tool.get_mut("function").and_then(Value::as_object_mut) {
                if function.get("strict") == Some(&Value::Bool(false)) {
                    function.remove("strict");
                }
                if function.get("description") == Some(&Value::Null) {
                    function.remove("description");
                }
            }
        }
    }
    if let Some(messages) = wire.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            let message = message
                .as_object_mut()
                .ok_or_else(|| error::invalid("hosted message is malformed"))?;
            if message.get("role").and_then(Value::as_str) == Some("assistant") {
                message.entry("content").or_insert(Value::Null);
            }
            // These services document system messages as their instruction role.
            if message.get("role").and_then(Value::as_str) == Some("developer") {
                message.insert("role".to_owned(), Value::String("system".to_owned()));
            }
            if let Some(parts) = message.get("content").and_then(Value::as_array)
                && parts.iter().all(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            {
                let text = parts
                    .iter()
                    .map(|part| part.get("text").and_then(Value::as_str).unwrap_or_default())
                    .collect::<String>();
                message.insert("content".to_owned(), Value::String(text));
            }
        }
    }
    Ok(())
}

pub(super) fn replay(
    replay: &ReasoningReplay,
    service: Option<HostedService>,
) -> Result<Map<String, Value>, ProviderCoreError> {
    let service = service
        .ok_or_else(|| error::invalid("reasoning replay requires a named hosted contract"))?;
    let value: Value = serde_json::from_slice(replay.opaque_for_wire())
        .map_err(|_| error::invalid("hosted reasoning replay is malformed"))?;
    if value.get("service").and_then(Value::as_str) != Some(service.name()) {
        return Err(error::invalid("reasoning replay belongs to a different hosted service"));
    }
    let fields = value
        .get("fields")
        .and_then(Value::as_object)
        .ok_or_else(|| error::invalid("hosted reasoning replay omitted its fields"))?;
    for (name, value) in fields {
        if !crate::hosted_reasoning::accepts(service, name)
            || !(value.is_string() || name == "reasoning_details" && value.is_array())
        {
            return Err(error::invalid("hosted reasoning replay contains an unmapped field"));
        }
    }
    Ok(fields.clone())
}

/// Preserve required-choice acceptance semantics without disabling provider-default thinking.
/// `DeepSeek` and other thinking models reject forced wire choices. Offer only the named tool
/// with automatic choice, then require the exact call in the hosted stream decoder.
fn tool_choice(
    wire: &mut Map<String, Value>,
    choice: &ToolChoice,
) -> Result<(), ProviderCoreError> {
    match choice {
        ToolChoice::Specific(name) => {
            let tools = wire
                .get_mut("tools")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| error::invalid("required tool is missing"))?;
            tools.retain(|tool| {
                tool.pointer("/function/name").and_then(Value::as_str) == Some(name.as_str())
            });
            if tools.len() != 1 {
                return Err(error::invalid("required tool is missing or repeated"));
            }
            wire.insert("tool_choice".to_owned(), Value::String("auto".to_owned()));
        }
        ToolChoice::Required => {
            wire.insert("tool_choice".to_owned(), Value::String("auto".to_owned()));
        }
        _ => {}
    }
    Ok(())
}
