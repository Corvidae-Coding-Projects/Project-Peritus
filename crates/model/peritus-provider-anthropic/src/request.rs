//! Checked provider-neutral request to private Anthropic Messages projection.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use peritus_model_protocol::{
    CachePolicy, Capability, ContentBlock, MediaInput, MediaKind, MediaReferenceKind, ModelRequest,
    ParallelToolPolicy, ReasoningEffort, ReasoningPolicy, Role, SchemaDialect, StructuredOutput,
    SummaryPolicy, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use crate::config::{AnthropicBeta, AnthropicConfig};
use crate::wire::{WireMessage, WireRequest};

mod value;

use value::{choice_value, json_value, millionths, typed_value, wire_object, wire_string};

pub fn encode(
    request: &ModelRequest,
    config: &AnthropicConfig,
) -> Result<Vec<u8>, ProviderCoreError> {
    validate_controls(request)?;
    let mut system = Vec::new();
    let mut messages = Vec::new();
    for message in request.messages() {
        match message.role() {
            Role::System | Role::Developer => {
                for block in message.content() {
                    system.push(system_block(block)?);
                }
            }
            role => messages.push(WireMessage {
                role: message_role(role)?,
                content: message
                    .content()
                    .iter()
                    .map(|block| content_block(block, config))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
        }
    }
    if messages.is_empty() {
        return Err(invalid("Anthropic Messages requires at least one user or assistant message"));
    }
    apply_cache(request.options().cache(), &mut system, &mut messages)?;
    let tools = request.tools().iter().map(tool).collect::<Result<Vec<_>, _>>()?;
    let (thinking, effort) = thinking(request.options().reasoning())?;
    let output_config = output_config(request.options().output(), effort)?;
    let generation = request.options().generation();
    let wire = WireRequest {
        model: request.model().as_str().to_owned(),
        max_tokens: generation.max_output_tokens(),
        messages,
        stream: true,
        system: (!system.is_empty()).then_some(system),
        tools: (!tools.is_empty()).then_some(tools),
        tool_choice: Some(tool_choice(request.tool_choice(), request.parallel_tool_policy())),
        stop_sequences: (!generation.stop_sequences().is_empty()).then(|| {
            generation
                .stop_sequences()
                .iter()
                .map(|stop| stop.expose_for_wire().to_owned())
                .collect()
        }),
        temperature: generation.temperature_millionths().map(millionths),
        top_p: generation.top_p_millionths().map(millionths),
        thinking,
        output_config,
    };
    serde_json::to_vec(&wire.into_value()).map_err(|_| {
        ProviderCoreError::invalid_request(
            "anthropic_encode",
            "Anthropic request serialization failed",
        )
    })
}

fn validate_controls(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(invalid(
            "Anthropic streaming must be selected because Messages requests force stream=true",
        ));
    }
    let generation = request.options().generation();
    if generation.seed().is_some() {
        return Err(invalid("Anthropic Messages does not support deterministic seed control"));
    }
    if !matches!(request.options().reasoning(), ReasoningPolicy::Disabled)
        && (generation.temperature_millionths().is_some()
            || generation.top_p_millionths().is_some())
    {
        return Err(invalid("Anthropic adaptive thinking cannot be combined with sampling"));
    }
    if request.options().persistence().store()
        || request.options().persistence().background()
        || request.options().continuation().is_some()
        || !request.options().extensions().is_empty()
    {
        return Err(invalid(
            "Anthropic Messages has no stored, background, cursor, or request-extension contract",
        ));
    }
    Ok(())
}

fn system_block(block: &ContentBlock) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(text) => Ok(wire_object([
            ("type", wire_string("text")),
            ("text", wire_string(text.expose_for_wire())),
        ])),
        _ => Err(invalid("Anthropic top-level system instructions must contain only text")),
    }
}

const fn message_role(role: Role) -> Result<&'static str, ProviderCoreError> {
    match role {
        Role::User | Role::Tool => Ok("user"),
        Role::Assistant => Ok("assistant"),
        Role::System | Role::Developer => {
            Err(invalid("system roles must be projected outside Anthropic messages"))
        }
    }
}

fn content_block(
    block: &ContentBlock,
    config: &AnthropicConfig,
) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(text) | ContentBlock::Refusal(text) => Ok(wire_object([
            ("type", wire_string("text")),
            ("text", wire_string(text.expose_for_wire())),
        ])),
        ContentBlock::Image(media) => media_block(media, MediaKind::Image, config),
        ContentBlock::Document(media) => media_block(media, MediaKind::Document, config),
        ContentBlock::Audio(_) => Err(invalid("Anthropic Messages does not support audio input")),
        ContentBlock::ToolCall(call) => Ok(wire_object([
            ("type", wire_string("tool_use")),
            ("id", wire_string(call.id().expose_for_wire())),
            ("name", wire_string(call.name().as_str())),
            ("input", json_value(call.arguments().canonical_bytes())?),
        ])),
        ContentBlock::ToolResult(result) => Ok(wire_object([
            ("type", wire_string("tool_result")),
            ("tool_use_id", wire_string(result.call_id().expose_for_wire())),
            ("content", wire_string(&result.output().to_wire_string())),
            ("is_error", Value::Bool(result.is_error())),
        ])),
        ContentBlock::Reasoning(replay) => reasoning_replay(replay),
        ContentBlock::ProviderExtension(_) => {
            Err(invalid("Anthropic request provider extensions are not profile-authorized"))
        }
    }
}

fn media_block(
    media: &MediaInput,
    expected: MediaKind,
    config: &AnthropicConfig,
) -> Result<Value, ProviderCoreError> {
    if media.kind() != expected {
        return Err(invalid("media semantic kind does not match its content block"));
    }
    let source = if let Some(bytes) = media.inline_bytes_for_wire() {
        wire_object([
            ("type", wire_string("base64")),
            ("media_type", wire_string(media.media_type().as_str())),
            ("data", Value::String(STANDARD.encode(bytes))),
        ])
    } else if let Some((kind, reference)) = media.reference_for_wire() {
        match kind {
            MediaReferenceKind::HttpsUrl => {
                wire_object([("type", wire_string("url")), ("url", wire_string(reference))])
            }
            MediaReferenceKind::ProviderFile
                if config.has_beta(AnthropicBeta::FilesApi20250414) =>
            {
                wire_object([("type", wire_string("file")), ("file_id", wire_string(reference))])
            }
            MediaReferenceKind::ProviderFile => {
                return Err(invalid(
                    "Anthropic provider files require the configured Files API beta",
                ));
            }
        }
    } else {
        return Err(invalid("Anthropic cannot read Peritus artifact references directly"));
    };
    Ok(wire_object([
        ("type", wire_string(if expected == MediaKind::Image { "image" } else { "document" })),
        ("source", source),
    ]))
}

fn reasoning_replay(
    replay: &peritus_model_protocol::ReasoningReplay,
) -> Result<Value, ProviderCoreError> {
    let text = core::str::from_utf8(replay.opaque_for_wire())
        .map_err(|_| invalid("Anthropic reasoning replay state is not UTF-8"))?;
    let canonical = peritus_model_protocol::CanonicalJson::parse(
        text,
        peritus_model_protocol::JsonBounds::value(
            peritus_model_protocol::ProtocolLimits::PRODUCTION,
        ),
    )
    .map_err(|_| invalid("Anthropic reasoning replay state is malformed or unbounded"))?;
    let value = json_value(canonical.canonical_bytes())?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("Anthropic reasoning replay state must be an object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("thinking")
            if object.keys().all(|key| matches!(key.as_str(), "type" | "signature")) =>
        {
            let signature = object
                .get("signature")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Anthropic thinking replay signature is missing"))?;
            let summary = replay
                .summary()
                .ok_or_else(|| invalid("Anthropic thinking replay text is missing"))?;
            Ok(wire_object([
                ("type", wire_string("thinking")),
                ("thinking", wire_string(summary.expose_for_wire())),
                ("signature", wire_string(signature)),
            ]))
        }
        Some("redacted_thinking")
            if object.keys().all(|key| matches!(key.as_str(), "type" | "data"))
                && replay.summary().is_none() =>
        {
            object
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Anthropic redacted-thinking replay data is missing"))?;
            Ok(value)
        }
        _ => Err(invalid("Anthropic reasoning replay state has an unsupported shape")),
    }
}

fn tool(tool: &peritus_model_protocol::ToolDefinition) -> Result<Value, ProviderCoreError> {
    if tool.parameters().dialect() != SchemaDialect::Draft202012 {
        return Err(invalid("Anthropic tool schemas require JSON Schema Draft 2020-12"));
    }
    let mut value = Map::new();
    value.insert("name".to_owned(), Value::String(tool.name().as_str().to_owned()));
    if let Some(description) = tool.description() {
        value.insert(
            "description".to_owned(),
            Value::String(description.expose_for_wire().to_owned()),
        );
    }
    value.insert("input_schema".to_owned(), json_value(tool.parameters().canonical_bytes())?);
    value.insert("strict".to_owned(), Value::Bool(tool.strict()));
    Ok(Value::Object(value))
}

fn tool_choice(choice: &ToolChoice, parallel: ParallelToolPolicy) -> Value {
    let disabled = matches!(parallel, ParallelToolPolicy::Disabled);
    match choice {
        ToolChoice::Auto => choice_value("auto", disabled, None),
        ToolChoice::None => choice_value("none", disabled, None),
        ToolChoice::Required => choice_value("any", disabled, None),
        ToolChoice::Specific(name) => choice_value("tool", disabled, Some(name.as_str())),
    }
}

fn thinking(
    policy: ReasoningPolicy,
) -> Result<(Option<Value>, Option<&'static str>), ProviderCoreError> {
    match policy {
        ReasoningPolicy::Disabled => Ok((None, None)),
        ReasoningPolicy::Adaptive { summary } => {
            validate_summary(summary)?;
            Ok((Some(typed_value("adaptive")), None))
        }
        ReasoningPolicy::Effort { effort, summary } => {
            validate_summary(summary)?;
            let effort = match effort {
                ReasoningEffort::Minimal | ReasoningEffort::Low => "low",
                ReasoningEffort::Medium => "medium",
                ReasoningEffort::High => "high",
            };
            Ok((Some(typed_value("adaptive")), Some(effort)))
        }
    }
}

const fn validate_summary(summary: SummaryPolicy) -> Result<(), ProviderCoreError> {
    if matches!(summary, SummaryPolicy::Concise | SummaryPolicy::Detailed) {
        return Err(invalid("Anthropic adaptive thinking does not expose summary length control"));
    }
    Ok(())
}

fn output_config(
    output: &StructuredOutput,
    effort: Option<&'static str>,
) -> Result<Option<Value>, ProviderCoreError> {
    let format = match output {
        StructuredOutput::Text => None,
        StructuredOutput::JsonObject => Some(wire_object([
            ("type", wire_string("json_schema")),
            ("schema", typed_value("object")),
        ])),
        StructuredOutput::JsonSchema { schema, strict, .. } => {
            if !strict || schema.dialect() != SchemaDialect::Draft202012 {
                return Err(invalid(
                    "Anthropic structured output requires strict JSON Schema Draft 2020-12",
                ));
            }
            Some(wire_object([
                ("type", wire_string("json_schema")),
                ("schema", json_value(schema.canonical_bytes())?),
            ]))
        }
    };
    if format.is_none() && effort.is_none() {
        return Ok(None);
    }
    let mut config = Map::new();
    if let Some(format) = format {
        config.insert("format".to_owned(), format);
    }
    if let Some(effort) = effort {
        config.insert("effort".to_owned(), Value::String(effort.to_owned()));
    }
    Ok(Some(Value::Object(config)))
}

fn apply_cache(
    cache: &CachePolicy,
    system: &mut [Value],
    messages: &mut [WireMessage],
) -> Result<(), ProviderCoreError> {
    let ttl = match cache {
        CachePolicy::Disabled | CachePolicy::Automatic => return Ok(()),
        CachePolicy::Ephemeral { ttl_seconds: 300 } => "5m",
        CachePolicy::Ephemeral { ttl_seconds: 3600 } => "1h",
        CachePolicy::Ephemeral { .. } => {
            return Err(invalid("Anthropic cache TTL must be exactly five minutes or one hour"));
        }
        CachePolicy::Explicit(_) => {
            return Err(invalid("Anthropic Messages has no explicit cache-key reuse contract"));
        }
    };
    let target = messages
        .last_mut()
        .and_then(|message| message.content.last_mut())
        .or_else(|| system.last_mut())
        .ok_or_else(|| invalid("Anthropic cache breakpoint has no content target"))?;
    target
        .as_object_mut()
        .ok_or_else(|| invalid("Anthropic cache target is not an object"))?
        .insert(
            "cache_control".to_owned(),
            wire_object([("type", wire_string("ephemeral")), ("ttl", wire_string(ttl))]),
        );
    Ok(())
}

const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::invalid_request("anthropic_request", detail)
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
