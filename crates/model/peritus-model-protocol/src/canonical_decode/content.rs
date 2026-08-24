//! Checked message, content-block, and media decoding.

use peritus_codec::CanonicalReader;
use peritus_types::{ArtifactId, Sha256Digest};

use super::primitive::{
    bounded_text, canonical_json, codec, invalid, optional_digest, optional_text,
    read_collection_len, unknown_tag,
};
use crate::{
    CompletedToolCall, ContentBlock, ExtensionName, MediaInput, MediaKind, MediaReferenceKind,
    MediaType, Message, ProtocolError, ProtocolLimits, ProviderExtension, ReasoningReplay, Role,
    ToolCallId, ToolName, ToolResult,
};

pub(super) fn message(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<Message, ProtocolError> {
    let role = match reader.read_u8().map_err(codec)? {
        1 => Role::System,
        2 => Role::Developer,
        3 => Role::User,
        4 => Role::Assistant,
        5 => Role::Tool,
        _ => return Err(unknown_tag("message.role")),
    };
    let count = read_collection_len(reader, limits.max_content_blocks(), "message.content")?;
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        blocks.push(block(reader, limits)?);
    }
    Message::new(role, blocks, limits)
}

fn block(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ContentBlock, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => bounded_text(reader, limits).map(ContentBlock::Text),
        2 => media(reader, MediaKind::Image, limits).map(ContentBlock::Image),
        3 => media(reader, MediaKind::Audio, limits).map(ContentBlock::Audio),
        4 => media(reader, MediaKind::Document, limits).map(ContentBlock::Document),
        5 => tool_call(reader, limits).map(ContentBlock::ToolCall),
        6 => tool_result(reader, limits).map(ContentBlock::ToolResult),
        7 => bounded_text(reader, limits).map(ContentBlock::Refusal),
        8 => reasoning(reader, limits).map(ContentBlock::Reasoning),
        9 => extension(reader, limits).map(ContentBlock::ProviderExtension),
        _ => Err(unknown_tag("content_block")),
    }
}

fn media(
    reader: &mut CanonicalReader<'_>,
    expected_kind: MediaKind,
    limits: ProtocolLimits,
) -> Result<MediaInput, ProtocolError> {
    let kind = match reader.read_u8().map_err(codec)? {
        1 => MediaKind::Image,
        2 => MediaKind::Audio,
        3 => MediaKind::Document,
        _ => return Err(unknown_tag("media.kind")),
    };
    if kind != expected_kind {
        return Err(invalid("media.kind", "content and media kind tags disagree"));
    }
    let media_type = MediaType::new(reader.read_str().map_err(codec)?.to_owned())?;
    match reader.read_u8().map_err(codec)? {
        1 => {
            MediaInput::inline(kind, media_type, reader.read_bytes_owned().map_err(codec)?, limits)
        }
        2 => referenced_media(reader, kind, media_type),
        3 => artifact_media(reader, kind, media_type),
        _ => Err(unknown_tag("media.source")),
    }
}

fn referenced_media(
    reader: &mut CanonicalReader<'_>,
    kind: MediaKind,
    media_type: MediaType,
) -> Result<MediaInput, ProtocolError> {
    let reference_kind = match reader.read_u8().map_err(codec)? {
        1 => MediaReferenceKind::HttpsUrl,
        2 => MediaReferenceKind::ProviderFile,
        _ => return Err(unknown_tag("media.reference_kind")),
    };
    let value = reader.read_str().map_err(codec)?.to_owned();
    let digest = optional_digest(reader)?;
    MediaInput::referenced(kind, media_type, reference_kind, value, digest)
}

fn artifact_media(
    reader: &mut CanonicalReader<'_>,
    kind: MediaKind,
    media_type: MediaType,
) -> Result<MediaInput, ProtocolError> {
    let artifact_id = ArtifactId::new(reader.read_fixed::<16>().map_err(codec)?)
        .map_err(|_| invalid("media.artifact_id", "artifact identity is invalid"))?;
    let digest = Sha256Digest::new(reader.read_fixed::<32>().map_err(codec)?);
    Ok(MediaInput::artifact(kind, media_type, artifact_id, digest))
}

fn tool_call(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<CompletedToolCall, ProtocolError> {
    let id = ToolCallId::new(reader.read_str().map_err(codec)?.to_owned())?;
    let name = ToolName::new(reader.read_str().map_err(codec)?.to_owned())?;
    CompletedToolCall::new(id, name, canonical_json(reader, limits)?)
}

fn tool_result(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ToolResult, ProtocolError> {
    let call_id = ToolCallId::new(reader.read_str().map_err(codec)?.to_owned())?;
    let is_error = reader.read_bool().map_err(codec)?;
    Ok(ToolResult::new(call_id, canonical_json(reader, limits)?, is_error))
}

fn reasoning(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ReasoningReplay, ProtocolError> {
    let summary = optional_text(reader, limits)?;
    let opaque = reader.read_bytes_owned().map_err(codec)?;
    ReasoningReplay::new(summary, opaque, limits)
}

pub(super) fn extension(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ProviderExtension, ProtocolError> {
    let name = ExtensionName::new(reader.read_str().map_err(codec)?.to_owned())?;
    Ok(ProviderExtension::new(name, canonical_json(reader, limits)?))
}
