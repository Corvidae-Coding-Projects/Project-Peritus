//! Separate documented ChatGPT-plan Responses wire profile; API-key encoding stays unchanged.

use peritus_model_protocol::{CachePolicy, ContentBlock, Message, ModelRequest, Role, ToolChoice};
use peritus_provider_core::ProviderCoreError;
use serde_json::json;

use super::{input, options};
use crate::error;

pub(super) fn encode(request: &ModelRequest) -> Result<Vec<u8>, ProviderCoreError> {
    super::validate_common(request)?;
    if request.options().persistence().store()
        || request.options().persistence().background()
        || request.options().continuation().is_some()
        || request.options().generation().temperature_millionths().is_some()
        || request.options().generation().top_p_millionths().is_some()
        || matches!(request.options().cache(), CachePolicy::Ephemeral { .. })
        || request
            .messages()
            .iter()
            .flat_map(Message::content)
            .any(|block| matches!(block, ContentBlock::Audio(_)))
    {
        return Err(error::invalid(
            "ChatGPT-plan OAuth requires streamed stateless replay without unsupported sampling, TTL, continuation or audio controls",
        ));
    }
    if request.messages().iter().any(|message| {
        message.role() == Role::System
            && message.content().iter().any(|block| !matches!(block, ContentBlock::Text(_)))
    }) {
        return Err(error::invalid("ChatGPT system instructions must contain only text"));
    }
    let mut messages = input::messages(request)?;
    let mut instructions = Vec::new();
    messages.retain(|message| {
        if message["role"] != "system" {
            return true;
        }
        if let Some(content) = message["content"].as_array() {
            for part in content {
                if let Some(text) = part["text"].as_str() {
                    instructions.push(text.to_owned());
                }
            }
        }
        false
    });
    for message in &mut messages {
        if matches!(message["type"].as_str(), Some("function_call" | "function_call_output")) {
            message["namespace"] = json!("peritus");
        }
    }
    let mut tools = options::tools(request)?;
    if let ToolChoice::Specific(name) = request.tool_choice() {
        tools.retain(|tool| tool["name"].as_str() == Some(name.as_str()));
    }
    let mut body = json!({
        "model":request.model().as_str(), "input":messages,
        "stream":true, "store":false, "text":options::text(request)?,
        "parallel_tool_calls":options::parallel(request),
    });
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions.join("\n\n"));
    }
    if !tools.is_empty() {
        body["tools"] = json!([{"type":"namespace", "name":"peritus",
            "description":"Peritus host tools. Calls are validated and executed by Peritus under current user permissions.", "tools":tools}]);
    }
    if let Some(choice) = options::tool_choice(request) {
        body["tool_choice"] = if matches!(request.tool_choice(), ToolChoice::Specific(_)) {
            json!("required")
        } else {
            choice
        };
    }
    if let Some(reasoning) = options::reasoning(request) {
        body["reasoning"] = reasoning;
    }
    let includes = input::reasoning_includes(request);
    if !includes.is_empty() {
        body["include"] = json!(includes);
    }
    if let Some(key) = options::cache_key(request) {
        body["prompt_cache_key"] = json!(key);
    }
    serde_json::to_vec(&body).map_err(|_| error::invalid("ChatGPT request serialization failed"))
}
