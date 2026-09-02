//! Local text and image rubric completion through the credential-owning Codex router.

use base64::Engine as _;
use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, MediaInput, MediaKind,
    MediaType, Message, ModelRequest, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ReasoningPolicy, ReducedItem, RequestId, RequestOptions, RequestedCapabilities,
    ResponseReducer, Role, StructuredOutput, TerminalOutcome, ToolChoice, negotiate,
};
use peritus_provider_core::{CancellationToken, ModelProvider};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{BenchmarkError, providers};

const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    temperature: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatMessage {
    role: String,
    content: Value,
}

pub async fn complete(body: &[u8]) -> Result<Value, BenchmarkError> {
    if body.len() > MAX_REQUEST_BYTES {
        return Err(BenchmarkError::Arguments("rubric request exceeds 32 MiB".to_owned()));
    }
    let request: ChatRequest = serde_json::from_slice(body)?;
    if request.model.trim().is_empty() || request.messages.is_empty() {
        return Err(BenchmarkError::Arguments(
            "rubric request requires a model and messages".to_owned(),
        ));
    }
    if request.temperature.is_some_and(|value| !value.is_finite()) {
        return Err(BenchmarkError::Arguments("rubric temperature must be finite".to_owned()));
    }
    let limits = ProtocolLimits::PRODUCTION;
    let messages = request
        .messages
        .iter()
        .map(|message| protocol_message(message, limits))
        .collect::<Result<Vec<_>, _>>()?;
    let required_capabilities = if messages.iter().any(message_has_image) {
        vec![Capability::ImageInput]
    } else {
        Vec::new()
    };
    let cancellation = CancellationToken::new();
    let provider = providers::codex_authenticated(&cancellation).await?;
    let profile = provider.profile();
    let requested = RequestedCapabilities::new(
        &required_capabilities,
        &[Capability::Streaming],
        profile.limits(),
    )
    .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let negotiated = negotiate(profile, requested)
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let model_request = ModelRequest::new(
        profile,
        negotiated,
        request_id(body)?,
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(8_192, Vec::new(), None, None, None)
                .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let mut stream = provider
        .start(model_request, cancellation)
        .await
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    let mut reducer = ResponseReducer::new(profile.provider().clone(), limits);
    while let Some(event) =
        stream.pull().await.map_err(|error| BenchmarkError::Provider(error.to_string()))?
    {
        reducer.push(event).map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    }
    response(&reducer)
}

fn response(reducer: &ResponseReducer) -> Result<Value, BenchmarkError> {
    if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
        return Err(BenchmarkError::Provider(format!(
            "rubric provider terminal was {:?}",
            reducer.terminal()
        )));
    }
    let mut content = String::new();
    for item in reducer.completed_items() {
        match item {
            ReducedItem::Text { text, .. } => content.push_str(text.expose_for_wire()),
            ReducedItem::Refusal { .. } => {
                return Err(BenchmarkError::Provider(
                    "rubric provider refused the request".to_owned(),
                ));
            }
            _ => {}
        }
    }
    if content.trim().is_empty() {
        return Err(BenchmarkError::Provider("rubric provider returned no usable text".to_owned()));
    }
    let usage = reducer.usage_high_water();
    let input = usage.input_tokens().unwrap_or(0);
    let output = usage.output_tokens().unwrap_or(0);
    let total = usage.total_tokens().unwrap_or_else(|| input.saturating_add(output));
    Ok(json!({
        "id": "peritus-local-rubric",
        "object": "chat.completion",
        "created": 0,
        "model": providers::WRITER_MODEL,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": input,
            "completion_tokens": output,
            "total_tokens": total,
            "cached_input_tokens": usage.cached_input_tokens().unwrap_or(0)
        }
    }))
}

fn protocol_message(
    message: &ChatMessage,
    limits: ProtocolLimits,
) -> Result<Message, BenchmarkError> {
    let role = match message.role.as_str() {
        "system" => Role::System,
        "developer" => Role::Developer,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        _ => {
            return Err(BenchmarkError::Arguments(format!(
                "unsupported rubric message role {:?}",
                message.role
            )));
        }
    };
    let content = content_blocks(&message.content, limits)?;
    Message::new(role, content, limits).map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn content_blocks(
    content: &Value,
    limits: ProtocolLimits,
) -> Result<Vec<ContentBlock>, BenchmarkError> {
    if let Some(text) = content.as_str() {
        return Ok(vec![text_block(text, limits)?]);
    }
    let parts = content.as_array().ok_or_else(|| {
        BenchmarkError::Arguments("rubric message content is neither text nor parts".to_owned())
    })?;
    let mut blocks = Vec::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => {
                let value = part.get("text").and_then(Value::as_str).ok_or_else(|| {
                    BenchmarkError::Arguments("rubric text part has no text".to_owned())
                })?;
                blocks.push(text_block(value, limits)?);
            }
            Some("image_url") => {
                blocks.push(ContentBlock::Image(image_block(part, limits)?));
            }
            other => {
                return Err(BenchmarkError::Arguments(format!(
                    "unsupported rubric content part {other:?}"
                )));
            }
        }
    }
    if blocks.is_empty() {
        return Err(BenchmarkError::Arguments("rubric message has no content".to_owned()));
    }
    Ok(blocks)
}

fn text_block(value: &str, limits: ProtocolLimits) -> Result<ContentBlock, BenchmarkError> {
    BoundedText::new(value.to_owned(), limits)
        .map(ContentBlock::Text)
        .map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn image_block(part: &Value, limits: ProtocolLimits) -> Result<MediaInput, BenchmarkError> {
    let url = part
        .get("image_url")
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
        .ok_or_else(|| BenchmarkError::Arguments("rubric image part has no URL".to_owned()))?;
    let data = url.strip_prefix("data:").ok_or_else(|| {
        BenchmarkError::Arguments("rubric image must use an inline data URL".to_owned())
    })?;
    let (media_type, encoded) = data.split_once(";base64,").ok_or_else(|| {
        BenchmarkError::Arguments("rubric image data URL is not base64 encoded".to_owned())
    })?;
    if !matches!(media_type, "image/png" | "image/jpeg" | "image/webp" | "image/gif") {
        return Err(BenchmarkError::Arguments(
            "rubric image type must be PNG, JPEG, WebP, or GIF".to_owned(),
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| BenchmarkError::Arguments("rubric image base64 is invalid".to_owned()))?;
    let media_type = MediaType::new(media_type.to_owned())
        .map_err(|error| BenchmarkError::Provider(error.to_string()))?;
    MediaInput::inline(MediaKind::Image, media_type, bytes, limits)
        .map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn message_has_image(message: &Message) -> bool {
    message.content().iter().any(|block| matches!(block, ContentBlock::Image(_)))
}

fn request_id(body: &[u8]) -> Result<RequestId, BenchmarkError> {
    let digest = Sha256::digest(body);
    let mut value = String::from("peritus-rubric-");
    for byte in &digest[..12] {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    RequestId::new(value).map_err(|error| BenchmarkError::Provider(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_image_parts_project_to_protocol_blocks() {
        let limits = ProtocolLimits::PRODUCTION;
        let plain = content_blocks(&json!("plain"), limits).unwrap();
        assert!(matches!(&plain[..], [ContentBlock::Text(_)]));
        let parts = content_blocks(
            &json!([
                {"type": "text", "text": "first"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,aW1hZ2U="}},
                {"type": "text", "text": "second"}
            ]),
            limits,
        )
        .unwrap();
        assert!(matches!(
            &parts[..],
            [ContentBlock::Text(_), ContentBlock::Image(_), ContentBlock::Text(_)]
        ));
        match &parts[1] {
            ContentBlock::Image(media) => assert_eq!(media.inline_len(), 5),
            _ => panic!("second block is not an image"),
        }
        assert!(content_blocks(
            &json!([{"type": "image_url", "image_url": {"url": "https://example.invalid/a.png"}}]),
            limits,
        )
        .is_err());
    }

    #[test]
    fn request_rejects_unknown_fields() {
        let value = serde_json::from_value::<ChatRequest>(json!({
            "model": "model",
            "messages": [],
            "surprise": true
        }));
        assert!(value.is_err());
    }
}
