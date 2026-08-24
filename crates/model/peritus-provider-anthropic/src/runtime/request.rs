//! Deterministic transcript and inert structured-result projection for Claude.

use std::collections::BTreeSet;

use peritus_model_protocol::{
    CachePolicy, ContentBlock, ModelRequest, ParallelToolPolicy, ReasoningPolicy, Role,
    StructuredOutput, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

const SYSTEM_PREFIX: &str = "You are the inference backend inside Peritus. Peritus is the sole agent harness and authority for tools, policy, approvals, and conversation state. Claude Code native tools, plugins, hooks, MCP servers, and session state are not the Peritus tool interface. Return only the next assistant turn through the required structured output. A non-empty tool_calls array requests inert host operations; never execute them yourself.\n\n";
const INPUT_PREFIX: &str = "The following JSON is the complete ordered conversation state owned by Peritus. Tool definitions were moved into the required structured-output schema. The max_output_tokens value is advisory because this runtime exposes no exact output-token control. Return only the next assistant turn.\n\n";

pub(super) struct RuntimeRequest {
    pub system: Vec<u8>,
    pub prompt: Vec<u8>,
    pub schema: String,
    pub allowed_tools: BTreeSet<String>,
    pub max_calls: usize,
}

pub(super) fn encode(request: &ModelRequest) -> Result<RuntimeRequest, ProviderCoreError> {
    validate_controls(request)?;
    let mut system = String::from(SYSTEM_PREFIX);
    let mut messages = Vec::new();
    for message in request.messages() {
        match message.role() {
            Role::System | Role::Developer => append_system(&mut system, message)?,
            role => messages.push(object([
                ("role", Value::String(role_name(role).to_owned())),
                (
                    "content",
                    Value::Array(
                        message.content().iter().map(content).collect::<Result<Vec<_>, _>>()?,
                    ),
                ),
            ])),
        }
    }
    if messages.is_empty() {
        return Err(invalid("Claude runtime requires at least one non-system message"));
    }
    let payload = object([
        ("model", Value::String(request.model().as_str().to_owned())),
        ("max_output_tokens", Value::from(request.options().generation().max_output_tokens())),
        ("messages", Value::Array(messages)),
    ]);
    let mut prompt = INPUT_PREFIX.as_bytes().to_vec();
    prompt.extend_from_slice(
        &serde_json::to_vec(&payload)
            .map_err(|_| invalid("Claude runtime transcript serialization failed"))?,
    );
    let (schema, allowed_tools, max_calls) = result_schema(request)?;
    Ok(RuntimeRequest { system: system.into_bytes(), prompt, schema, allowed_tools, max_calls })
}

fn validate_controls(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    let generation = request.options().generation();
    if !matches!(request.options().output(), StructuredOutput::Text)
        || request.options().reasoning() != ReasoningPolicy::Disabled
        || !matches!(request.options().cache(), CachePolicy::Disabled)
        || request.options().persistence().store()
        || request.options().persistence().background()
        || request.options().continuation().is_some()
        || !request.options().extensions().is_empty()
        || !generation.stop_sequences().is_empty()
        || generation.seed().is_some()
        || generation.temperature_millionths().is_some()
        || generation.top_p_millionths().is_some()
    {
        return Err(invalid(
            "Claude runtime supports text output, advisory length, disabled reasoning/cache, and local replay only",
        ));
    }
    Ok(())
}

fn append_system(
    output: &mut String,
    message: &peritus_model_protocol::Message,
) -> Result<(), ProviderCoreError> {
    let label = if message.role() == Role::System { "system" } else { "developer" };
    for block in message.content() {
        let ContentBlock::Text(text) = block else {
            return Err(invalid("Claude runtime system instructions must be text"));
        };
        output.push('[');
        output.push_str(label);
        output.push_str("]\n");
        output.push_str(text.expose_for_wire());
        output.push_str("\n\n");
    }
    Ok(())
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::User | Role::Tool => "user",
        Role::Assistant => "assistant",
        Role::System | Role::Developer => "system",
    }
}

fn content(block: &ContentBlock) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(text) | ContentBlock::Refusal(text) => Ok(object([
            ("type", Value::String("text".to_owned())),
            ("text", Value::String(text.expose_for_wire().to_owned())),
        ])),
        ContentBlock::ToolCall(call) => Ok(object([
            ("type", Value::String("tool_use".to_owned())),
            ("id", Value::String(call.id().expose_for_wire().to_owned())),
            ("name", Value::String(call.name().as_str().to_owned())),
            ("input", json_value(call.arguments().canonical_bytes())?),
        ])),
        ContentBlock::ToolResult(result) => Ok(object([
            ("type", Value::String("tool_result".to_owned())),
            ("tool_use_id", Value::String(result.call_id().expose_for_wire().to_owned())),
            ("content", json_value(result.output().canonical_bytes())?),
            ("is_error", Value::Bool(result.is_error())),
        ])),
        ContentBlock::Image(_)
        | ContentBlock::Audio(_)
        | ContentBlock::Document(_)
        | ContentBlock::Reasoning(_)
        | ContentBlock::ProviderExtension(_) => {
            Err(invalid("Claude runtime accepts only text and host tool history"))
        }
    }
}

fn result_schema(
    request: &ModelRequest,
) -> Result<(String, BTreeSet<String>, usize), ProviderCoreError> {
    let selected = match request.tool_choice() {
        ToolChoice::Specific(name) => Some(name.as_str()),
        ToolChoice::Auto | ToolChoice::None | ToolChoice::Required => None,
    };
    let mut names = BTreeSet::new();
    let mut variants = Vec::new();
    for tool in request
        .tools()
        .iter()
        .filter(|tool| selected.is_none_or(|selected| selected == tool.name().as_str()))
    {
        if !names.insert(tool.name().as_str().to_owned()) {
            return Err(invalid("Claude runtime tool names must be unique"));
        }
        let parameters = json_value(tool.parameters().canonical_bytes())?;
        let name = object([
            ("type", Value::String("string".to_owned())),
            ("const", Value::String(tool.name().as_str().to_owned())),
        ]);
        let properties = object([("name", name), ("arguments", parameters)]);
        let mut variant = object([
            ("type", Value::String("object".to_owned())),
            ("properties", properties),
            (
                "required",
                Value::Array(vec![
                    Value::String("name".to_owned()),
                    Value::String("arguments".to_owned()),
                ]),
            ),
            ("additionalProperties", Value::Bool(false)),
        ]);
        if let Some(description) = tool.description() {
            variant["description"] = Value::String(description.expose_for_wire().to_owned());
        }
        variants.push(variant);
    }
    if selected.is_some() && variants.is_empty() {
        return Err(invalid("Claude runtime selected tool is absent"));
    }
    let maximum = match (request.tool_choice(), request.parallel_tool_policy()) {
        (ToolChoice::None, _) => 0,
        (_, ParallelToolPolicy::Disabled) => usize::from(!variants.is_empty()),
        (_, ParallelToolPolicy::Allowed(count)) => usize::try_from(count)
            .map_err(|_| invalid("parallel tool limit is not representable"))?,
    };
    let minimum = usize::from(matches!(
        request.tool_choice(),
        ToolChoice::Required | ToolChoice::Specific(_)
    ));
    let items = if variants.is_empty() {
        object([("type", Value::String("object".to_owned()))])
    } else {
        object([("oneOf", Value::Array(variants))])
    };
    let content =
        object([("type", Value::String("string".to_owned())), ("minLength", Value::from(1_u64))]);
    let calls = object([
        ("type", Value::String("array".to_owned())),
        ("minItems", number(minimum)?),
        ("maxItems", number(maximum)?),
        ("items", items),
    ]);
    let schema = object([
        ("type", Value::String("object".to_owned())),
        ("properties", object([("content", content), ("tool_calls", calls)])),
        (
            "required",
            Value::Array(vec![
                Value::String("content".to_owned()),
                Value::String("tool_calls".to_owned()),
            ]),
        ),
        ("additionalProperties", Value::Bool(false)),
    ]);
    let schema = serde_json::to_string(&schema)
        .map_err(|_| invalid("Claude runtime output schema serialization failed"))?;
    Ok((schema, names, maximum))
}

fn json_value(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes).map_err(|_| invalid("canonical JSON could not be projected"))
}

fn object<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Object(
        entries.into_iter().map(|(name, value)| (name.to_owned(), value)).collect::<Map<_, _>>(),
    )
}

fn number(value: usize) -> Result<Value, ProviderCoreError> {
    let value = u64::try_from(value).map_err(|_| invalid("schema limit is not representable"))?;
    Ok(Value::from(value))
}

const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::invalid_request("claude_runtime_request", detail)
}
