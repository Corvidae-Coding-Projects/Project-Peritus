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
    if generation.seed().is_some() || !generation.stop_sequences().is_empty() {
        return Err(error::invalid("Responses compatibility does not map seed or stop sequences"));
    }
    let mut wire = Map::new();
    wire.insert("model".to_owned(), string(request.model().as_str()));
    wire.insert("input".to_owned(), Value::Array(messages(request)?));
    wire.insert("stream".to_owned(), Value::Bool(true));
    wire.insert("store".to_owned(), Value::Bool(false));
    wire.insert("background".to_owned(), Value::Bool(false));
    wire.insert("max_output_tokens".to_owned(), Value::from(generation.max_output_tokens()));
    add_tools(&mut wire, request)?;
    wire.insert("parallel_tool_calls".to_owned(), Value::Bool(parallel(request)));
    wire.insert("text".to_owned(), text_format(request)?);
    if let Some(value) = generation.temperature_millionths() {
        wire.insert("temperature".to_owned(), Value::from(f64::from(value) / 1_000_000.0));
    }
    if let Some(value) = generation.top_p_millionths() {
        wire.insert("top_p".to_owned(), Value::from(f64::from(value) / 1_000_000.0));
    }
    serde_json::to_vec(&Value::Object(wire))
        .map_err(|_| error::invalid("Responses-compatible request serialization failed"))
}

fn messages(request: &ModelRequest) -> Result<Vec<Value>, ProviderCoreError> {
    let mut items = Vec::new();
    for message in request.messages() {
        project_message(message, &mut items)?;
    }
    Ok(items)
}

fn project_message(message: &Message, items: &mut Vec<Value>) -> Result<(), ProviderCoreError> {
    let mut parts = Vec::new();
    for block in message.content() {
        match block {
            ContentBlock::Text(text) => parts.push(object([
                ("text", string(text.expose_for_wire())),
                (
                    "type",
                    string(if message.role() == Role::Assistant {
                        "output_text"
                    } else {
                        "input_text"
                    }),
                ),
            ])),
            ContentBlock::Image(media) => parts.push(image(media)?),
            ContentBlock::Refusal(text) => parts.push(object([
                ("refusal", string(text.expose_for_wire())),
                ("type", string("refusal")),
            ])),
            ContentBlock::ToolCall(call) => {
                flush(message.role(), &mut parts, items);
                items.push(object([
                    ("arguments", string(&call.arguments().to_wire_string())),
                    ("call_id", string(call.id().expose_for_wire())),
                    ("name", string(call.name().as_str())),
                    ("type", string("function_call")),
                ]));
            }
            ContentBlock::ToolResult(result) => {
                flush(message.role(), &mut parts, items);
                let output = object([
                    ("is_error", Value::Bool(result.is_error())),
                    ("output", canonical(result.output().canonical_bytes())?),
                ]);
                let output = serde_json::to_string(&output)
                    .map_err(|_| error::invalid("compatible tool result serialization failed"))?;
                items.push(object([
                    ("call_id", string(result.call_id().expose_for_wire())),
                    ("output", string(&output)),
                    ("type", string("function_call_output")),
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
    flush(message.role(), &mut parts, items);
    Ok(())
}

fn flush(role: Role, parts: &mut Vec<Value>, items: &mut Vec<Value>) {
    if !parts.is_empty() {
        items.push(object([
            ("content", Value::Array(core::mem::take(parts))),
            ("role", string(role_name(role))),
            ("type", string("message")),
        ]));
    }
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::Developer => "developer",
        Role::User | Role::Tool => "user",
        Role::Assistant => "assistant",
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
    Ok(object([("image_url", string(&url)), ("type", string("input_image"))]))
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
                        "description",
                        optional_string(
                            tool.description()
                                .map(peritus_model_protocol::BoundedText::expose_for_wire),
                        ),
                    ),
                    ("name", string(tool.name().as_str())),
                    ("parameters", canonical(tool.parameters().canonical_bytes())?),
                    ("strict", Value::Bool(tool.strict())),
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
            ToolChoice::Specific(name) => {
                object([("name", string(name.as_str())), ("type", string("function"))])
            }
        };
        wire.insert("tool_choice".to_owned(), choice);
    }
    Ok(())
}

const fn parallel(request: &ModelRequest) -> bool {
    matches!(request.parallel_tool_policy(), ParallelToolPolicy::Allowed(_))
}

fn text_format(request: &ModelRequest) -> Result<Value, ProviderCoreError> {
    let format = match request.options().output() {
        StructuredOutput::Text => object([("type", string("text"))]),
        StructuredOutput::JsonObject => object([("type", string("json_object"))]),
        StructuredOutput::JsonSchema { name, schema, strict } => object([
            ("name", string(name.as_str())),
            ("schema", canonical(schema.canonical_bytes())?),
            ("strict", Value::Bool(*strict)),
            ("type", string("json_schema")),
        ]),
    };
    Ok(object([("format", format)]))
}
