//! Stable semantic request encoding independent of provider JSON field order.

use peritus_codec::{CanonicalWriter, CodecLimits};

use crate::{
    CachePolicy, ContentBlock, MediaKind, MediaReferenceKind, Message, ModelRequest,
    ParallelToolPolicy, ProtocolError, ProtocolErrorKind, ReasoningEffort, ReasoningPolicy, Role,
    StructuredOutput, SummaryPolicy, ToolChoice, WireDialect,
};

const CANONICAL_MAGIC: &[u8; 4] = b"P5MR";

pub fn request_bytes(request: &ModelRequest) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = CanonicalWriter::new(CodecLimits::new(
        512 * 1024 * 1024,
        512 * 1024 * 1024,
        1_100_000,
        32 * 1024 * 1024,
        256 * 1024 * 1024,
        128,
    ));
    write(&mut writer, CANONICAL_MAGIC)?;
    u16_value(&mut writer, request.protocol().major())?;
    u16_value(&mut writer, request.protocol().minor())?;
    write(&mut writer, request.profile_id().as_bytes())?;
    u64_value(&mut writer, request.profile_revision())?;
    text(&mut writer, request.provider().as_str())?;
    u8_value(&mut writer, dialect(request.dialect()))?;
    text(&mut writer, request.model().as_str())?;
    u64_value(&mut writer, request.negotiated().selected_mask())?;
    let limits = request.negotiated().limits();
    u64_value(&mut writer, limits.max_input_tokens())?;
    u64_value(&mut writer, limits.max_output_tokens())?;
    u32_value(&mut writer, limits.max_tools())?;
    u32_value(&mut writer, limits.max_parallel_tool_calls())?;
    u64_value(&mut writer, limits.max_inline_media_bytes())?;

    collection(&mut writer, request.messages().len())?;
    for message in request.messages() {
        message_value(&mut writer, message)?;
    }

    collection(&mut writer, request.tools().len())?;
    for tool in request.tools() {
        text(&mut writer, tool.name().as_str())?;
        optional_text(&mut writer, tool.description().map(crate::BoundedText::expose_for_wire))?;
        u8_value(&mut writer, schema_dialect(tool.parameters().dialect()))?;
        bytes(&mut writer, tool.parameters().canonical_bytes())?;
        boolean(&mut writer, tool.strict())?;
    }
    tool_choice(&mut writer, request.tool_choice())?;
    parallel_policy(&mut writer, request.parallel_tool_policy())?;

    let options = request.options();
    output_policy(&mut writer, options.output())?;
    reasoning_policy(&mut writer, options.reasoning())?;
    let generation = options.generation();
    u64_value(&mut writer, generation.max_output_tokens())?;
    collection(&mut writer, generation.stop_sequences().len())?;
    for stop in generation.stop_sequences() {
        text(&mut writer, stop.expose_for_wire())?;
    }
    optional_i64(&mut writer, generation.seed())?;
    optional_u32(&mut writer, generation.temperature_millionths())?;
    optional_u32(&mut writer, generation.top_p_millionths())?;
    cache_policy(&mut writer, options.cache())?;
    boolean(&mut writer, options.persistence().store())?;
    boolean(&mut writer, options.persistence().background())?;
    option_tag(&mut writer, options.continuation().is_some())?;
    if let Some(continuation) = options.continuation() {
        text(&mut writer, continuation.response_id().expose_for_wire())?;
        optional_text(&mut writer, continuation.event_id().map(crate::EventId::expose_for_wire))?;
        optional_u64(&mut writer, continuation.sequence())?;
    }
    collection(&mut writer, options.extensions().len())?;
    for extension in options.extensions() {
        text(&mut writer, extension.name().as_str())?;
        bytes(&mut writer, extension.value().canonical_bytes())?;
    }
    Ok(writer.into_bytes())
}

fn message_value(writer: &mut CanonicalWriter, message: &Message) -> Result<(), ProtocolError> {
    u8_value(
        writer,
        match message.role() {
            Role::System => 1,
            Role::Developer => 2,
            Role::User => 3,
            Role::Assistant => 4,
            Role::Tool => 5,
        },
    )?;
    collection(writer, message.content().len())?;
    for block in message.content() {
        content(writer, block)?;
    }
    Ok(())
}

fn content(writer: &mut CanonicalWriter, block: &ContentBlock) -> Result<(), ProtocolError> {
    match block {
        ContentBlock::Text(value) => {
            u8_value(writer, 1)?;
            text(writer, value.expose_for_wire())
        }
        ContentBlock::Image(media) => media_value(writer, 2, media),
        ContentBlock::Audio(media) => media_value(writer, 3, media),
        ContentBlock::Document(media) => media_value(writer, 4, media),
        ContentBlock::ToolCall(call) => {
            u8_value(writer, 5)?;
            text(writer, call.id().expose_for_wire())?;
            text(writer, call.name().as_str())?;
            bytes(writer, call.arguments().canonical_bytes())
        }
        ContentBlock::ToolResult(result) => {
            u8_value(writer, 6)?;
            text(writer, result.call_id().expose_for_wire())?;
            boolean(writer, result.is_error())?;
            bytes(writer, result.output().canonical_bytes())
        }
        ContentBlock::Refusal(value) => {
            u8_value(writer, 7)?;
            text(writer, value.expose_for_wire())
        }
        ContentBlock::Reasoning(value) => {
            u8_value(writer, 8)?;
            optional_text(writer, value.summary().map(crate::BoundedText::expose_for_wire))?;
            bytes(writer, value.opaque_for_wire())
        }
        ContentBlock::ProviderExtension(extension) => {
            u8_value(writer, 9)?;
            text(writer, extension.name().as_str())?;
            bytes(writer, extension.value().canonical_bytes())
        }
    }
}

fn media_value(
    writer: &mut CanonicalWriter,
    tag: u8,
    media: &crate::MediaInput,
) -> Result<(), ProtocolError> {
    u8_value(writer, tag)?;
    u8_value(
        writer,
        match media.kind() {
            MediaKind::Image => 1,
            MediaKind::Audio => 2,
            MediaKind::Document => 3,
        },
    )?;
    text(writer, media.media_type().as_str())?;
    if let Some(value) = media.inline_bytes_for_wire() {
        u8_value(writer, 1)?;
        bytes(writer, value)?;
    } else if let Some((kind, value)) = media.reference_for_wire() {
        u8_value(writer, 2)?;
        u8_value(
            writer,
            match kind {
                MediaReferenceKind::HttpsUrl => 1,
                MediaReferenceKind::ProviderFile => 2,
            },
        )?;
        text(writer, value)?;
        optional_digest(writer, media.digest())?;
    } else if let Some((artifact_id, digest)) = media.artifact_reference() {
        u8_value(writer, 3)?;
        write(writer, artifact_id.as_bytes())?;
        write(writer, digest.as_bytes())?;
    } else {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidRequest,
            "media",
            "media source is not representable",
        ));
    }
    Ok(())
}

fn tool_choice(writer: &mut CanonicalWriter, choice: &ToolChoice) -> Result<(), ProtocolError> {
    match choice {
        ToolChoice::Auto => u8_value(writer, 1),
        ToolChoice::None => u8_value(writer, 2),
        ToolChoice::Required => u8_value(writer, 3),
        ToolChoice::Specific(name) => {
            u8_value(writer, 4)?;
            text(writer, name.as_str())
        }
    }
}

fn parallel_policy(
    writer: &mut CanonicalWriter,
    policy: ParallelToolPolicy,
) -> Result<(), ProtocolError> {
    match policy {
        ParallelToolPolicy::Disabled => u8_value(writer, 1),
        ParallelToolPolicy::Allowed(count) => {
            u8_value(writer, 2)?;
            u32_value(writer, count)
        }
    }
}

fn output_policy(
    writer: &mut CanonicalWriter,
    output: &StructuredOutput,
) -> Result<(), ProtocolError> {
    match output {
        StructuredOutput::Text => u8_value(writer, 1),
        StructuredOutput::JsonObject => u8_value(writer, 2),
        StructuredOutput::JsonSchema { name, schema, strict } => {
            u8_value(writer, 3)?;
            text(writer, name.as_str())?;
            u8_value(writer, schema_dialect(schema.dialect()))?;
            bytes(writer, schema.canonical_bytes())?;
            boolean(writer, *strict)
        }
    }
}

fn reasoning_policy(
    writer: &mut CanonicalWriter,
    reasoning: ReasoningPolicy,
) -> Result<(), ProtocolError> {
    match reasoning {
        ReasoningPolicy::Disabled => u8_value(writer, 1),
        ReasoningPolicy::Adaptive { summary } => {
            u8_value(writer, 2)?;
            u8_value(writer, summary_policy(summary))
        }
        ReasoningPolicy::Effort { effort, summary } => {
            u8_value(writer, 3)?;
            u8_value(
                writer,
                match effort {
                    ReasoningEffort::Minimal => 1,
                    ReasoningEffort::Low => 2,
                    ReasoningEffort::Medium => 3,
                    ReasoningEffort::High => 4,
                },
            )?;
            u8_value(writer, summary_policy(summary))
        }
    }
}

fn cache_policy(writer: &mut CanonicalWriter, cache: &CachePolicy) -> Result<(), ProtocolError> {
    match cache {
        CachePolicy::Disabled => u8_value(writer, 1),
        CachePolicy::Automatic => u8_value(writer, 2),
        CachePolicy::Ephemeral { ttl_seconds } => {
            u8_value(writer, 3)?;
            u32_value(writer, *ttl_seconds)
        }
        CachePolicy::Explicit(key) => {
            u8_value(writer, 4)?;
            text(writer, key.expose_for_wire())
        }
    }
}

const fn dialect(value: WireDialect) -> u8 {
    match value {
        WireDialect::OpenAiResponses => 1,
        WireDialect::AnthropicMessages => 2,
        WireDialect::GeminiInteractionsV1 => 3,
        WireDialect::GeminiGenerateContentV1 => 4,
        WireDialect::CompatibleResponses => 5,
        WireDialect::CompatibleChatCompletions => 6,
        WireDialect::OpenAiCodexRuntime => 7,
        WireDialect::AnthropicClaudeRuntime => 8,
    }
}

const fn schema_dialect(value: crate::SchemaDialect) -> u8 {
    match value {
        crate::SchemaDialect::Draft202012 => 1,
        crate::SchemaDialect::Draft7 => 2,
        crate::SchemaDialect::GeminiSubset => 3,
        crate::SchemaDialect::ProfiledSubset => 4,
    }
}

const fn summary_policy(value: SummaryPolicy) -> u8 {
    match value {
        SummaryPolicy::None => 1,
        SummaryPolicy::Auto => 2,
        SummaryPolicy::Concise => 3,
        SummaryPolicy::Detailed => 4,
    }
}

fn optional_text(writer: &mut CanonicalWriter, value: Option<&str>) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        text(writer, value)?;
    }
    Ok(())
}

fn optional_digest(
    writer: &mut CanonicalWriter,
    value: Option<peritus_types::Sha256Digest>,
) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        write(writer, value.as_bytes())?;
    }
    Ok(())
}

fn optional_u64(writer: &mut CanonicalWriter, value: Option<u64>) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        u64_value(writer, value)?;
    }
    Ok(())
}

fn optional_i64(writer: &mut CanonicalWriter, value: Option<i64>) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        write(writer, &value.to_be_bytes())?;
    }
    Ok(())
}

fn optional_u32(writer: &mut CanonicalWriter, value: Option<u32>) -> Result<(), ProtocolError> {
    option_tag(writer, value.is_some())?;
    if let Some(value) = value {
        u32_value(writer, value)?;
    }
    Ok(())
}

fn collection(writer: &mut CanonicalWriter, value: usize) -> Result<(), ProtocolError> {
    writer.write_collection_len(value).map_err(codec)
}

fn text(writer: &mut CanonicalWriter, value: &str) -> Result<(), ProtocolError> {
    writer.write_str(value).map_err(codec)
}

fn bytes(writer: &mut CanonicalWriter, value: &[u8]) -> Result<(), ProtocolError> {
    writer.write_bytes(value).map_err(codec)
}

fn write(writer: &mut CanonicalWriter, value: &[u8]) -> Result<(), ProtocolError> {
    writer.write_fixed(value).map_err(codec)
}

fn boolean(writer: &mut CanonicalWriter, value: bool) -> Result<(), ProtocolError> {
    writer.write_bool(value).map_err(codec)
}

fn option_tag(writer: &mut CanonicalWriter, present: bool) -> Result<(), ProtocolError> {
    writer.write_option_tag(present).map_err(codec)
}

fn u8_value(writer: &mut CanonicalWriter, value: u8) -> Result<(), ProtocolError> {
    writer.write_u8(value).map_err(codec)
}

fn u16_value(writer: &mut CanonicalWriter, value: u16) -> Result<(), ProtocolError> {
    writer.write_u16(value).map_err(codec)
}

fn u32_value(writer: &mut CanonicalWriter, value: u32) -> Result<(), ProtocolError> {
    writer.write_u32(value).map_err(codec)
}

fn u64_value(writer: &mut CanonicalWriter, value: u64) -> Result<(), ProtocolError> {
    writer.write_u64(value).map_err(codec)
}

fn codec(_: peritus_codec::CodecError) -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidLimit,
        "canonical_request",
        "canonical request encoding exceeded an internal bound",
    )
}
