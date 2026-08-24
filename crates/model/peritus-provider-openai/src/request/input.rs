//! Ordered message, content, media, tool-result, and replay projection.

use base64::Engine as _;
use peritus_model_protocol::{
    ContentBlock, MediaInput, MediaKind, MediaReferenceKind, Message, ModelRequest,
    ReasoningReplay, Role,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::value::{object, string};
use crate::error;

pub(super) fn validate(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    for message in request.messages() {
        for block in message.content() {
            match block {
                ContentBlock::Image(media) if media.kind() != MediaKind::Image => {
                    return Err(error::invalid("image block contains another media kind"));
                }
                ContentBlock::Audio(media) if media.kind() != MediaKind::Audio => {
                    return Err(error::invalid("audio block contains another media kind"));
                }
                ContentBlock::Document(media) if media.kind() != MediaKind::Document => {
                    return Err(error::invalid("document block contains another media kind"));
                }
                ContentBlock::Audio(media)
                    if media.inline_bytes_for_wire().is_none()
                        || !matches!(media.media_type().as_str(), "audio/wav" | "audio/mpeg") =>
                {
                    return Err(error::invalid(
                        "OpenAI audio input requires inline WAV or MPEG bytes",
                    ));
                }
                ContentBlock::ProviderExtension(_) => {
                    return Err(error::invalid(
                        "first-party OpenAI profiles do not accept provider extensions",
                    ));
                }
                ContentBlock::Text(_)
                | ContentBlock::Image(_)
                | ContentBlock::Audio(_)
                | ContentBlock::Document(_)
                | ContentBlock::ToolCall(_)
                | ContentBlock::ToolResult(_)
                | ContentBlock::Refusal(_)
                | ContentBlock::Reasoning(_) => {}
            }
        }
    }
    Ok(())
}

pub(super) fn messages(request: &ModelRequest) -> Result<Vec<Value>, ProviderCoreError> {
    let mut items = Vec::new();
    for message in request.messages() {
        project_message(message, &mut items)?;
    }
    Ok(items)
}

pub(super) fn reasoning_includes(request: &ModelRequest) -> Vec<&'static str> {
    request
        .messages()
        .iter()
        .flat_map(Message::content)
        .any(|block| matches!(block, ContentBlock::Reasoning(_)))
        .then_some("reasoning.encrypted_content")
        .into_iter()
        .collect()
}

fn project_message(message: &Message, items: &mut Vec<Value>) -> Result<(), ProviderCoreError> {
    let mut message_parts = Vec::new();
    for block in message.content() {
        match block {
            ContentBlock::Text(text) => message_parts.push(object([
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
            ContentBlock::Image(media) => message_parts.push(image(media)?),
            ContentBlock::Audio(media) => message_parts.push(audio(media)?),
            ContentBlock::Document(media) => message_parts.push(document(media)?),
            ContentBlock::Refusal(text) => message_parts.push(object([
                ("refusal", string(text.expose_for_wire())),
                ("type", string("refusal")),
            ])),
            ContentBlock::ToolCall(call) => {
                flush_message(message.role(), &mut message_parts, items);
                items.push(object([
                    ("arguments", string(&call.arguments().to_wire_string())),
                    ("call_id", string(call.id().expose_for_wire())),
                    ("name", string(call.name().as_str())),
                    ("type", string("function_call")),
                ]));
            }
            ContentBlock::ToolResult(result) => {
                flush_message(message.role(), &mut message_parts, items);
                let output = object([
                    ("is_error", Value::Bool(result.is_error())),
                    ("output", canonical(result.output().canonical_bytes())?),
                ]);
                let output = serde_json::to_string(&output)
                    .map_err(|_| error::invalid("tool result serialization failed"))?;
                items.push(object([
                    ("call_id", string(result.call_id().expose_for_wire())),
                    ("output", string(&output)),
                    ("type", string("function_call_output")),
                ]));
            }
            ContentBlock::Reasoning(replay) => {
                flush_message(message.role(), &mut message_parts, items);
                items.push(reasoning(replay)?);
            }
            ContentBlock::ProviderExtension(_) => {
                return Err(error::invalid("provider extension reached OpenAI encoding"));
            }
        }
    }
    flush_message(message.role(), &mut message_parts, items);
    Ok(())
}

fn flush_message(role: Role, parts: &mut Vec<Value>, items: &mut Vec<Value>) {
    if parts.is_empty() {
        return;
    }
    items.push(object([
        ("content", Value::Array(core::mem::take(parts))),
        ("role", string(role_name(role))),
        ("type", string("message")),
    ]));
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
    media_value(media, "input_image", "image_url", "file_id")
}

fn document(media: &MediaInput) -> Result<Value, ProviderCoreError> {
    media_value(media, "input_file", "file_url", "file_id")
}

fn media_value(
    media: &MediaInput,
    kind: &'static str,
    url_key: &'static str,
    file_key: &'static str,
) -> Result<Value, ProviderCoreError> {
    let mut value = Map::new();
    value.insert("type".to_owned(), Value::String(kind.to_owned()));
    if let Some(bytes) = media.inline_bytes_for_wire() {
        let data = format!(
            "data:{};base64,{}",
            media.media_type().as_str(),
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let key = if kind == "input_file" { "file_data" } else { url_key };
        value.insert(key.to_owned(), Value::String(data));
    } else if let Some((reference_kind, reference)) = media.reference_for_wire() {
        let key = match reference_kind {
            MediaReferenceKind::HttpsUrl => url_key,
            MediaReferenceKind::ProviderFile => file_key,
        };
        value.insert(key.to_owned(), Value::String(reference.to_owned()));
    } else {
        return Err(error::invalid(
            "Peritus artifact media must be resolved before OpenAI projection",
        ));
    }
    Ok(Value::Object(value))
}

fn audio(media: &MediaInput) -> Result<Value, ProviderCoreError> {
    let bytes = media
        .inline_bytes_for_wire()
        .ok_or_else(|| error::invalid("OpenAI audio input must be inline"))?;
    let format = match media.media_type().as_str() {
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        _ => return Err(error::invalid("OpenAI audio input format is unsupported")),
    };
    Ok(object([
        (
            "input_audio",
            object([
                ("data", string(&base64::engine::general_purpose::STANDARD.encode(bytes))),
                ("format", string(format)),
            ]),
        ),
        ("type", string("input_audio")),
    ]))
}

fn reasoning(replay: &ReasoningReplay) -> Result<Value, ProviderCoreError> {
    let encrypted = core::str::from_utf8(replay.opaque_for_wire())
        .map_err(|_| error::invalid("OpenAI reasoning replay must be a wire string"))?;
    let summary = replay.summary().map_or_else(Vec::new, |text| {
        vec![object([("text", string(text.expose_for_wire())), ("type", string("summary_text"))])]
    });
    Ok(object([
        ("encrypted_content", string(encrypted)),
        ("summary", Value::Array(summary)),
        ("type", string("reasoning")),
    ]))
}

fn canonical(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes)
        .map_err(|_| error::invalid("validated canonical JSON was invalid"))
}
