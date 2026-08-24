use base64::Engine as _;
use peritus_model_protocol::{
    ContentBlock, MediaInput, Message, ModelRequest, ParallelToolPolicy, Role, StructuredOutput,
    ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::validation::canonical;
use super::value::{object, optional_string, string};
use crate::error;

pub(super) fn encode(request: &ModelRequest) -> Result<Vec<u8>, ProviderCoreError> {
    let generation = request.options().generation();
    let mut wire = Map::new();
    wire.insert("model".to_owned(), string(request.model().as_str()));
    wire.insert("messages".to_owned(), Value::Array(messages(request)?));
    wire.insert("stream".to_owned(), Value::Bool(true));
    wire.insert("stream_options".to_owned(), object([("include_usage", Value::Bool(true))]));
    wire.insert("max_completion_tokens".to_owned(), Value::from(generation.max_output_tokens()));
    add_tools(&mut wire, request)?;
    wire.insert("parallel_tool_calls".to_owned(), Value::Bool(parallel(request)));
    wire.insert("response_format".to_owned(), response_format(request)?);
    if let Some(value) = generation.temperature_millionths() {
        wire.insert("temperature".to_owned(), Value::from(f64::from(value) / 1_000_000.0));
    }
    if let Some(value) = generation.top_p_millionths() {
        wire.insert("top_p".to_owned(), Value::from(f64::from(value) / 1_000_000.0));
    }
    if let Some(value) = generation.seed() {
        wire.insert("seed".to_owned(), Value::from(value));
    }
    if !generation.stop_sequences().is_empty() {
        wire.insert(
            "stop".to_owned(),
            Value::Array(
                generation.stop_sequences().iter().map(|v| string(v.expose_for_wire())).collect(),
            ),
        );
    }
    serde_json::to_vec(&Value::Object(wire))
        .map_err(|_| error::invalid("Chat-compatible request serialization failed"))
}

fn messages(request: &ModelRequest) -> Result<Vec<Value>, ProviderCoreError> {
    let mut values = Vec::new();
    for message in request.messages() {
        project_message(message, &mut values)?;
    }
    Ok(values)
}

fn project_message(message: &Message, values: &mut Vec<Value>) -> Result<(), ProviderCoreError> {
    let mut parts = Vec::new();
    let mut tool_calls = Vec::new();
    for block in message.content() {
        match block {
            ContentBlock::Text(text) | ContentBlock::Refusal(text) => parts
                .push(object([("text", string(text.expose_for_wire())), ("type", string("text"))])),
            ContentBlock::Image(media) => parts.push(image(media)?),
            ContentBlock::ToolCall(call) => tool_calls.push(object([
                (
                    "function",
                    object([
                        ("arguments", string(&call.arguments().to_wire_string())),
                        ("name", string(call.name().as_str())),
                    ]),
                ),
                ("id", string(call.id().expose_for_wire())),
                ("type", string("function")),
            ])),
            ContentBlock::ToolResult(result) => {
                let output = object([
                    ("is_error", Value::Bool(result.is_error())),
                    ("output", canonical(result.output().canonical_bytes())?),
                ]);
                let output = serde_json::to_string(&output)
                    .map_err(|_| error::invalid("compatible tool result serialization failed"))?;
                values.push(object([
                    ("content", string(&output)),
                    ("role", string("tool")),
                    ("tool_call_id", string(result.call_id().expose_for_wire())),
                ]));
            }
            ContentBlock::Audio(_)
            | ContentBlock::Document(_)
            | ContentBlock::Reasoning(_)
            | ContentBlock::ProviderExtension(_) => {
                return Err(error::invalid("unsupported compatible content reached encoding"));
            }
        }
    }
    if !parts.is_empty() || !tool_calls.is_empty() {
        let mut value = Map::new();
        value.insert("role".to_owned(), string(role_name(message.role())));
        if !parts.is_empty() {
            value.insert("content".to_owned(), Value::Array(parts));
        }
        if !tool_calls.is_empty() {
            value.insert("tool_calls".to_owned(), Value::Array(tool_calls));
        }
        values.push(Value::Object(value));
    }
    Ok(())
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::Developer => "developer",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn image(media: &MediaInput) -> Result<Value, ProviderCoreError> {
    let url = if let Some(bytes) = media.inline_bytes_for_wire() {
        format!(
            "data:{};base64,{}",
            media.media_type().as_str(),
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    } else if let Some((_, reference)) = media.reference_for_wire() {
        reference.to_owned()
    } else {
        return Err(error::invalid("compatible image input was unresolved"));
    };
    Ok(object([("image_url", object([("url", string(&url))])), ("type", string("image_url"))]))
}

fn add_tools(
    wire: &mut Map<String, Value>,
    request: &ModelRequest,
) -> Result<(), ProviderCoreError> {
    if !request.tools().is_empty() {
        let tools = request
            .tools()
            .iter()
            .map(|tool| {
                Ok(object([
                    (
                        "function",
                        object([
                            (
                                "description",
                                optional_string(
                                    tool.description()
                                        .map(peritus_model_protocol::BoundedText::expose_for_wire),
                                ),
                            ),
                            ("name", string(tool.name().as_str())),
                            ("parameters", canonical(tool.parameters().canonical_bytes())?),
                            ("strict", Value::Bool(tool.strict())),
                        ]),
                    ),
                    ("type", string("function")),
                ]))
            })
            .collect::<Result<Vec<_>, ProviderCoreError>>()?;
        wire.insert("tools".to_owned(), Value::Array(tools));
    }
    if !request.tools().is_empty() || !matches!(request.tool_choice(), ToolChoice::Auto) {
        let choice = match request.tool_choice() {
            ToolChoice::Auto => string("auto"),
            ToolChoice::None => string("none"),
            ToolChoice::Required => string("required"),
            ToolChoice::Specific(name) => object([
                ("function", object([("name", string(name.as_str()))])),
                ("type", string("function")),
            ]),
        };
        wire.insert("tool_choice".to_owned(), choice);
    }
    Ok(())
}

const fn parallel(request: &ModelRequest) -> bool {
    matches!(request.parallel_tool_policy(), ParallelToolPolicy::Allowed(_))
}

fn response_format(request: &ModelRequest) -> Result<Value, ProviderCoreError> {
    match request.options().output() {
        StructuredOutput::Text => Ok(object([("type", string("text"))])),
        StructuredOutput::JsonObject => Ok(object([("type", string("json_object"))])),
        StructuredOutput::JsonSchema { name, schema, strict } => Ok(object([
            (
                "json_schema",
                object([
                    ("name", string(name.as_str())),
                    ("schema", canonical(schema.canonical_bytes())?),
                    ("strict", Value::Bool(*strict)),
                ]),
            ),
            ("type", string("json_schema")),
        ])),
    }
}
