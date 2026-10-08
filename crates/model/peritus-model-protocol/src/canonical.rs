//! Stable semantic request encoding independent of provider JSON field order.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{
    CachePolicy, ContentBlock, MediaKind, MediaReferenceKind, Message, ModelRequest,
    ParallelToolPolicy, ProtocolError, ProtocolErrorKind, ReasoningEffort, ReasoningPolicy, Role,
    StructuredOutput, SummaryPolicy, ToolChoice, WireDialect,
};

mod values;
use values::{
    boolean, bytes, collection, option_tag, optional_digest, optional_i64, optional_text,
    optional_u32, optional_u64, text, u8_value, u16_value, u32_value, u64_value, write_fixed,
};

const CANONICAL_MAGIC: &[u8; 4] = b"P5MR";

pub fn request_bytes(request: &ModelRequest) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    request_value(&mut writer, request)?;
    Ok(writer.into_bytes())
}

pub fn request_bytes_bounded(
    request: &ModelRequest,
    maximum_bytes: usize,
) -> Result<Vec<u8>, ProtocolError> {
    if maximum_bytes == 0 {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidLimit,
            "canonical_request",
            "canonical byte ceiling must be positive",
        ));
    }
    let mut writer = CanonicalWriter::new(CodecLimits::new(
        maximum_bytes,
        maximum_bytes,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        CodecLimits::UNLIMITED_NESTING,
    ));
    request_value(&mut writer, request)?;
    Ok(writer.into_bytes())
}

pub fn request_digest(request: &ModelRequest) -> Result<Sha256Digest, ProtocolError> {
    let mut writer = DigestWriter::new();
    request_value(&mut writer, request)?;
    Ok(writer.finish())
}

fn request_value<W: CanonicalSink + ?Sized>(
    mut writer: &mut W,
    request: &ModelRequest,
) -> Result<(), ProtocolError> {
    request.validate_admission()?;
    write_fixed(&mut writer, CANONICAL_MAGIC)?;
    u16_value(&mut writer, request.protocol().major())?;
    u16_value(&mut writer, request.protocol().minor())?;
    write_fixed(&mut writer, request.profile_id().as_bytes())?;
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
    Ok(())
}

pub(crate) fn message_value<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    message: &Message,
) -> Result<(), ProtocolError> {
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

fn content<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    block: &ContentBlock,
) -> Result<(), ProtocolError> {
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

fn media_value<W: CanonicalSink + ?Sized>(
    writer: &mut W,
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
    if let Some((artifact_id, digest)) = media.artifact_reference() {
        u8_value(writer, 3)?;
        write_fixed(writer, artifact_id.as_bytes())?;
        write_fixed(writer, digest.as_bytes())?;
    } else if let Some(value) = media.inline_bytes_for_wire() {
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
    } else {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidRequest,
            "media",
            "media source is not representable",
        ));
    }
    Ok(())
}

fn tool_choice<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    choice: &ToolChoice,
) -> Result<(), ProtocolError> {
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

fn parallel_policy<W: CanonicalSink + ?Sized>(
    writer: &mut W,
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

fn output_policy<W: CanonicalSink + ?Sized>(
    writer: &mut W,
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

fn reasoning_policy<W: CanonicalSink + ?Sized>(
    writer: &mut W,
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
                    ReasoningEffort::XHigh => 5,
                    ReasoningEffort::Max => 6,
                    ReasoningEffort::Ultra => 7,
                },
            )?;
            u8_value(writer, summary_policy(summary))
        }
    }
}

fn cache_policy<W: CanonicalSink + ?Sized>(
    writer: &mut W,
    cache: &CachePolicy,
) -> Result<(), ProtocolError> {
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

pub(crate) trait CanonicalSink {
    fn write_fixed(&mut self, value: &[u8]) -> Result<(), ProtocolError>;
    fn write_bytes(&mut self, value: &[u8]) -> Result<(), ProtocolError>;
    fn write_str(&mut self, value: &str) -> Result<(), ProtocolError>;
    fn write_collection_len(&mut self, value: usize) -> Result<(), ProtocolError>;
}

impl<T: CanonicalSink + ?Sized> CanonicalSink for &mut T {
    fn write_fixed(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        (**self).write_fixed(value)
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        (**self).write_bytes(value)
    }

    fn write_str(&mut self, value: &str) -> Result<(), ProtocolError> {
        (**self).write_str(value)
    }

    fn write_collection_len(&mut self, value: usize) -> Result<(), ProtocolError> {
        (**self).write_collection_len(value)
    }
}

impl CanonicalSink for CanonicalWriter {
    fn write_fixed(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        CanonicalWriter::write_fixed(self, value).map_err(codec)
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        CanonicalWriter::write_bytes(self, value).map_err(codec)
    }

    fn write_str(&mut self, value: &str) -> Result<(), ProtocolError> {
        CanonicalWriter::write_str(self, value).map_err(codec)
    }

    fn write_collection_len(&mut self, value: usize) -> Result<(), ProtocolError> {
        CanonicalWriter::write_collection_len(self, value).map_err(codec)
    }
}

struct DigestWriter {
    digest: Sha256,
}

impl DigestWriter {
    fn new() -> Self {
        Self { digest: Sha256::new() }
    }

    fn finish(self) -> Sha256Digest {
        Sha256Digest::new(self.digest.finalize().into())
    }
}

impl CanonicalSink for DigestWriter {
    fn write_fixed(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        self.digest.update(value);
        Ok(())
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        let length = u32::try_from(value.len()).map_err(|_| representation())?;
        self.digest.update(length.to_be_bytes());
        self.digest.update(value);
        Ok(())
    }

    fn write_str(&mut self, value: &str) -> Result<(), ProtocolError> {
        self.write_bytes(value.as_bytes())
    }

    fn write_collection_len(&mut self, value: usize) -> Result<(), ProtocolError> {
        let length = u32::try_from(value).map_err(|_| representation())?;
        self.digest.update(length.to_be_bytes());
        Ok(())
    }
}

fn codec(_: peritus_codec::CodecError) -> ProtocolError {
    representation()
}

fn representation() -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidLimit,
        "canonical_request",
        "canonical request encoding exceeded a selected storage or format bound",
    )
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
