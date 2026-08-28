//! Text-only local rubric completion through the credential-owning Codex router.

use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, Message, ModelRequest,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ReasoningPolicy, ReducedItem, RequestId,
    RequestOptions, RequestedCapabilities, ResponseReducer, Role, StructuredOutput,
    TerminalOutcome, ToolChoice, negotiate,
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
    let cancellation = CancellationToken::new();
    let provider = providers::codex_authenticated(&cancellation).await?;
    let profile = provider.profile();
    let requested = RequestedCapabilities::new(&[], &[Capability::Streaming], profile.limits())
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
    let text = content_text(&message.content)?;
    Message::new(
        role,
        vec![ContentBlock::Text(
            BoundedText::new(text, limits)
                .map_err(|error| BenchmarkError::Provider(error.to_string()))?,
        )],
        limits,
    )
    .map_err(|error| BenchmarkError::Provider(error.to_string()))
}

fn content_text(content: &Value) -> Result<String, BenchmarkError> {
    if let Some(text) = content.as_str() {
        return Ok(text.to_owned());
    }
    let parts = content.as_array().ok_or_else(|| {
        BenchmarkError::Arguments("rubric message content is neither text nor parts".to_owned())
    })?;
    let mut text = String::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => {
                let value = part.get("text").and_then(Value::as_str).ok_or_else(|| {
                    BenchmarkError::Arguments("rubric text part has no text".to_owned())
                })?;
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(value);
            }
            Some("image_url") => {
                return Err(BenchmarkError::Arguments(
                    "the subscription-backed rubric router does not support image input".to_owned(),
                ));
            }
            other => {
                return Err(BenchmarkError::Arguments(format!(
                    "unsupported rubric content part {other:?}"
                )));
            }
        }
    }
    if text.is_empty() {
        return Err(BenchmarkError::Arguments("rubric message has no text content".to_owned()));
    }
    Ok(text)
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
    fn text_and_text_parts_project_without_images() {
        assert_eq!(content_text(&json!("plain")).unwrap(), "plain");
        assert_eq!(
            content_text(&json!([
                {"type": "text", "text": "first"},
                {"type": "text", "text": "second"}
            ]))
            .unwrap(),
            "first\nsecond"
        );
        assert!(
            content_text(&json!([{"type": "image_url", "image_url": {"url": "data:"}}])).is_err()
        );
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
