//! Deterministic transcript projection for the credential-owning Codex router.

mod schema;

use peritus_model_protocol::{
    CachePolicy, ContentBlock, MediaInput, MediaKind, ModelRequest, ReasoningEffort,
    ReasoningPolicy, Role, StructuredOutput, SummaryPolicy, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

const PROMPT_PREFIX: &str = "Peritus is the sole host agent, policy authority, and owner of conversation state. The JSON below is one complete provider request. Do not invoke Codex-native tools. Return only the object required by --output-schema. Entries in host_tools are inert proposals for Peritus to validate and execute; never execute them yourself. max_output_tokens_advisory is a requested ceiling, not a claim that this runtime enforces it.\n\nPERITUS_PROVIDER_REQUEST_JSON:\n";

pub struct RuntimeRequest {
    pub prompt: Vec<u8>,
    pub schema: Vec<u8>,
    pub allowed_tools: std::collections::BTreeSet<String>,
    pub max_calls: usize,
    images: Vec<RuntimeImage>,
    effort: &'static str,
}

pub(super) struct RuntimeImage {
    pub(super) media_type: String,
    pub(super) bytes: Vec<u8>,
}

impl RuntimeRequest {
    pub(super) fn images(&self) -> &[RuntimeImage] {
        &self.images
    }

    pub(crate) const fn reasoning_effort(&self) -> &'static str {
        self.effort
    }
}

pub fn encode(request: &ModelRequest) -> Result<RuntimeRequest, ProviderCoreError> {
    let effort = validate_controls(request)?;
    let mut images = Vec::new();
    let messages = request
        .messages()
        .iter()
        .map(|item| message(item, &mut images))
        .collect::<Result<Vec<_>, _>>()?;
    let tools = request.tools().iter().map(tool).collect::<Result<Vec<_>, _>>()?;
    let payload = object(vec![
        ("model", Value::String(request.model().as_str().to_owned())),
        (
            "max_output_tokens_advisory",
            Value::Number(request.options().generation().max_output_tokens().into()),
        ),
        ("messages", Value::Array(messages)),
        ("host_tools", Value::Array(tools)),
        ("host_tool_choice", tool_choice(request.tool_choice())),
    ]);
    let mut prompt = PROMPT_PREFIX.as_bytes().to_vec();
    let payload = serde_json::to_vec(&payload)
        .map_err(|_| invalid("Codex runtime transcript serialization failed"))?;
    prompt.extend_from_slice(&payload);
    let contract = schema::result_contract(request)?;
    Ok(RuntimeRequest {
        prompt,
        schema: contract.bytes,
        allowed_tools: contract.allowed_tools,
        max_calls: contract.max_calls,
        images,
        effort,
    })
}

fn validate_controls(request: &ModelRequest) -> Result<&'static str, ProviderCoreError> {
    let generation = request.options().generation();
    let effort = reasoning_effort(request.options().reasoning())?;
    if !matches!(request.options().output(), StructuredOutput::Text)
        || !matches!(request.options().cache(), CachePolicy::Disabled | CachePolicy::Automatic)
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
            "Codex runtime supports text output, advisory length, bounded effort, automatic or disabled cache, and local replay only",
        ));
    }
    Ok(effort)
}

const fn reasoning_effort(policy: ReasoningPolicy) -> Result<&'static str, ProviderCoreError> {
    match policy {
        ReasoningPolicy::Disabled => Ok("high"),
        ReasoningPolicy::Effort { effort, summary: SummaryPolicy::None } => Ok(match effort {
            ReasoningEffort::Minimal | ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        }),
        ReasoningPolicy::Adaptive { .. } | ReasoningPolicy::Effort { .. } => Err(invalid(
            "Codex runtime requires a concrete reasoning effort without a visible summary",
        )),
    }
}

fn message(
    message: &peritus_model_protocol::Message,
    images: &mut Vec<RuntimeImage>,
) -> Result<Value, ProviderCoreError> {
    let content = message
        .content()
        .iter()
        .map(|block| content(block, images))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(object(vec![
        ("role", Value::String(role_name(message.role()).to_owned())),
        ("content", Value::Array(content)),
    ]))
}

fn content(
    block: &ContentBlock,
    images: &mut Vec<RuntimeImage>,
) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(text) | ContentBlock::Refusal(text) => Ok(object(vec![
            ("type", Value::String("text".to_owned())),
            ("text", Value::String(text.expose_for_wire().to_owned())),
        ])),
        ContentBlock::ToolCall(call) => Ok(object(vec![
            ("type", Value::String("host_tool_call".to_owned())),
            ("id", Value::String(call.id().expose_for_wire().to_owned())),
            ("name", Value::String(call.name().as_str().to_owned())),
            ("arguments", json_value(call.arguments().canonical_bytes())?),
        ])),
        ContentBlock::ToolResult(result) => Ok(object(vec![
            ("type", Value::String("host_tool_result".to_owned())),
            ("call_id", Value::String(result.call_id().expose_for_wire().to_owned())),
            ("output", json_value(result.output().canonical_bytes())?),
            ("is_error", Value::Bool(result.is_error())),
        ])),
        ContentBlock::Image(media) => image(media, images),
        ContentBlock::Audio(_)
        | ContentBlock::Document(_)
        | ContentBlock::Reasoning(_)
        | ContentBlock::ProviderExtension(_) => {
            Err(invalid("Codex runtime accepts text, inline images, and host tool history"))
        }
    }
}

fn image(media: &MediaInput, images: &mut Vec<RuntimeImage>) -> Result<Value, ProviderCoreError> {
    if media.kind() != MediaKind::Image {
        return Err(invalid("Codex runtime image block contains another media kind"));
    }
    let bytes = media
        .inline_bytes_for_wire()
        .ok_or_else(|| invalid("Codex runtime image input must use bounded inline bytes"))?;
    let index = images.len();
    images.push(RuntimeImage {
        media_type: media.media_type().as_str().to_owned(),
        bytes: bytes.to_vec(),
    });
    let digest = media
        .digest()
        .ok_or_else(|| invalid("Codex runtime inline image has no content digest"))?;
    let mut digest_hex = String::with_capacity(64);
    for byte in digest.as_bytes() {
        use core::fmt::Write as _;
        let _ = write!(digest_hex, "{byte:02x}");
    }
    Ok(object(vec![
        ("type", Value::String("image_attachment".to_owned())),
        ("attachment_index", Value::Number(index.into())),
        ("media_type", Value::String(media.media_type().as_str().to_owned())),
        ("sha256", Value::String(digest_hex)),
    ]))
}

fn tool(tool: &peritus_model_protocol::ToolDefinition) -> Result<Value, ProviderCoreError> {
    let mut fields = Map::new();
    fields.insert("name".to_owned(), Value::String(tool.name().as_str().to_owned()));
    fields.insert("parameters".to_owned(), json_value(tool.parameters().canonical_bytes())?);
    fields.insert("strict".to_owned(), Value::Bool(tool.strict()));
    if let Some(description) = tool.description() {
        fields.insert(
            "description".to_owned(),
            Value::String(description.expose_for_wire().to_owned()),
        );
    }
    Ok(Value::Object(fields))
}

fn tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".to_owned()),
        ToolChoice::None => Value::String("none".to_owned()),
        ToolChoice::Required => Value::String("required".to_owned()),
        ToolChoice::Specific(name) => object(vec![
            ("type", Value::String("specific".to_owned())),
            ("name", Value::String(name.as_str().to_owned())),
        ]),
    }
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

fn json_value(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes).map_err(|_| invalid("canonical JSON could not be projected"))
}

pub(super) fn object(fields: Vec<(&str, Value)>) -> Value {
    Value::Object(fields.into_iter().map(|(key, value)| (key.to_owned(), value)).collect())
}

pub(super) const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::invalid_request("codex_runtime_request", detail)
}
