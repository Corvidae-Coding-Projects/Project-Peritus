//! Shared content, tool-schema, and opaque-thinking projection.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use peritus_model_protocol::{
    ContentBlock, MediaInput, MediaKind, MediaReferenceKind, ReasoningReplay, SchemaDialect,
    ToolDefinition,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::invalid;
use super::value::{object, parse, string};

pub(super) fn interaction_content(block: &ContentBlock) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(text) | ContentBlock::Refusal(text) => {
            Ok(object([("type", string("text")), ("text", string(text.expose_for_wire()))]))
        }
        ContentBlock::Image(media) => interaction_media(media, MediaKind::Image, "image"),
        ContentBlock::Audio(media) => interaction_media(media, MediaKind::Audio, "audio"),
        ContentBlock::Document(media) => interaction_media(media, MediaKind::Document, "document"),
        _ => Err(invalid("content block is not valid inside Google interaction content")),
    }
}

pub(super) fn generate_part(block: &ContentBlock) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(text) | ContentBlock::Refusal(text) => {
            Ok(object([("text", string(text.expose_for_wire()))]))
        }
        ContentBlock::Image(media) => generate_media(media, MediaKind::Image),
        ContentBlock::Audio(media) => generate_media(media, MediaKind::Audio),
        ContentBlock::Document(media) => generate_media(media, MediaKind::Document),
        ContentBlock::ToolCall(call) => {
            let mut function = Map::new();
            function.insert("id".to_owned(), string(call.id().expose_for_wire()));
            function.insert("name".to_owned(), string(call.name().as_str()));
            function.insert("args".to_owned(), parse(call.arguments().canonical_bytes())?);
            Ok(object([("functionCall", Value::Object(function))]))
        }
        ContentBlock::ToolResult(result) => {
            let mut function = Map::new();
            function.insert("id".to_owned(), string(result.call_id().expose_for_wire()));
            function.insert("name".to_owned(), string("peritus_function"));
            function.insert("response".to_owned(), parse(result.output().canonical_bytes())?);
            if result.is_error() {
                function.insert("isError".to_owned(), Value::Bool(true));
            }
            Ok(object([("functionResponse", Value::Object(function))]))
        }
        ContentBlock::Reasoning(replay) => generate_replay(replay),
        ContentBlock::ProviderExtension(_) => {
            Err(invalid("Google provider extension content is unsupported"))
        }
    }
}

pub(super) fn interaction_tool(tool: &ToolDefinition) -> Result<Value, ProviderCoreError> {
    let schema = checked_schema(tool)?;
    let mut value = Map::new();
    value.insert("type".to_owned(), string("function"));
    value.insert("name".to_owned(), string(tool.name().as_str()));
    value.insert("parameters".to_owned(), schema);
    if let Some(description) = tool.description() {
        value.insert("description".to_owned(), string(description.expose_for_wire()));
    }
    Ok(Value::Object(value))
}

pub(super) fn generate_tool(tool: &ToolDefinition) -> Result<Value, ProviderCoreError> {
    let schema = checked_schema(tool)?;
    let mut value = Map::new();
    value.insert("name".to_owned(), string(tool.name().as_str()));
    value.insert("parametersJsonSchema".to_owned(), schema);
    if let Some(description) = tool.description() {
        value.insert("description".to_owned(), string(description.expose_for_wire()));
    }
    Ok(Value::Object(value))
}

pub(super) fn interaction_replay(replay: &ReasoningReplay) -> Result<Value, ProviderCoreError> {
    let value = parse(replay.opaque_for_wire())?;
    let replay_object = value
        .as_object()
        .ok_or_else(|| invalid("Google interaction thought replay must be an object"))?;
    if replay_object.get("type").and_then(Value::as_str) != Some("thought")
        || replay_object.get("signature").and_then(Value::as_str).is_none()
        || !replay_object.keys().all(|key| matches!(key.as_str(), "type" | "signature"))
    {
        return Err(invalid("Google interaction thought replay has an unsupported shape"));
    }
    let mut value = replay_object.clone();
    if let Some(summary) = replay.summary() {
        value.insert(
            "summary".to_owned(),
            Value::Array(vec![object([
                ("type", string("text")),
                ("text", string(summary.expose_for_wire())),
            ])]),
        );
    }
    Ok(Value::Object(value))
}

fn checked_schema(tool: &ToolDefinition) -> Result<Value, ProviderCoreError> {
    if tool.parameters().dialect() != SchemaDialect::GeminiSubset {
        return Err(invalid("Google function schemas require the Gemini JSON Schema subset"));
    }
    let value = parse(tool.parameters().canonical_bytes())?;
    if tool.strict()
        && value.pointer("/additionalProperties").and_then(Value::as_bool) != Some(false)
    {
        return Err(invalid("strict Google function inputs require additionalProperties=false"));
    }
    Ok(value)
}

fn interaction_media(
    media: &MediaInput,
    expected: MediaKind,
    kind: &str,
) -> Result<Value, ProviderCoreError> {
    validate_kind(media, expected)?;
    let mut value = Map::new();
    value.insert("type".to_owned(), string(kind));
    value.insert("mime_type".to_owned(), string(media.media_type().as_str()));
    if let Some(bytes) = media.inline_bytes_for_wire() {
        value.insert("data".to_owned(), Value::String(STANDARD.encode(bytes)));
    } else if let Some((_reference_kind, reference)) = media.reference_for_wire() {
        value.insert("uri".to_owned(), string(reference));
    } else {
        return Err(invalid("Google cannot read Peritus artifact references directly"));
    }
    Ok(Value::Object(value))
}

fn generate_media(media: &MediaInput, expected: MediaKind) -> Result<Value, ProviderCoreError> {
    validate_kind(media, expected)?;
    if let Some(bytes) = media.inline_bytes_for_wire() {
        return Ok(object([(
            "inlineData",
            object([
                ("mimeType", string(media.media_type().as_str())),
                ("data", Value::String(STANDARD.encode(bytes))),
            ]),
        )]));
    }
    if let Some((kind, reference)) = media.reference_for_wire() {
        if !matches!(kind, MediaReferenceKind::HttpsUrl | MediaReferenceKind::ProviderFile) {
            return Err(invalid("Google media reference kind is unsupported"));
        }
        return Ok(object([(
            "fileData",
            object([
                ("mimeType", string(media.media_type().as_str())),
                ("fileUri", string(reference)),
            ]),
        )]));
    }
    Err(invalid("Google cannot read Peritus artifact references directly"))
}

fn generate_replay(replay: &ReasoningReplay) -> Result<Value, ProviderCoreError> {
    let value = parse(replay.opaque_for_wire())?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("Google Generate Content thought replay must be an object"))?;
    let signature = object
        .get("thoughtSignature")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Google Generate Content thought signature is missing"))?;
    if !object.keys().all(|key| key == "thoughtSignature") {
        return Err(invalid("Google Generate Content thought replay has an unsupported shape"));
    }
    let mut part = Map::new();
    part.insert("thoughtSignature".to_owned(), string(signature));
    if let Some(summary) = replay.summary() {
        part.insert("thought".to_owned(), Value::Bool(true));
        part.insert("text".to_owned(), string(summary.expose_for_wire()));
    }
    Ok(Value::Object(part))
}

fn validate_kind(media: &MediaInput, expected: MediaKind) -> Result<(), ProviderCoreError> {
    if media.kind() != expected {
        return Err(invalid("media semantic kind does not match its content block"));
    }
    Ok(())
}
