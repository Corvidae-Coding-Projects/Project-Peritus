//! Checked message, content-block, and media decoding.

use peritus_codec::CanonicalReader;
use peritus_types::{ArtifactId, Sha256Digest};

use super::primitive::{
    bounded_text, canonical_json, codec, extension_json, invalid, optional_digest, optional_text,
    read_collection_len, unknown_tag,
};
use super::DecodeBudget;
use crate::{
    CompletedToolCall, ContentBlock, ExtensionName, MediaInput, MediaKind, MediaReferenceKind,
    MediaType, Message, ProtocolError, ProtocolLimits, ProviderExtension, ReasoningReplay, Role,
    ToolCallId, ToolName, ToolResult,
};

pub(super) fn message(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
    budget: &mut DecodeBudget,
) -> Result<Message, ProtocolError> {
    let role = match reader.read_u8().map_err(codec)? {
        1 => Role::System,
        2 => Role::Developer,
        3 => Role::User,
        4 => Role::Assistant,
        5 => Role::Tool,
        _ => return Err(unknown_tag("message.role")),
    };
    let count = read_collection_len(reader, limits.max_content_blocks(), 1 + 4, "message.content")?;
    budget.charge_content_blocks(count)?;
    let mut blocks = reader.reserve_collection(count).map_err(codec)?;
    for _ in 0..count {
        blocks.push(block(reader, limits, budget)?);
    }
    Message::new(role, blocks, limits)
}

fn block(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
    budget: &mut DecodeBudget,
) -> Result<ContentBlock, ProtocolError> {
    match reader.read_u8().map_err(codec)? {
        1 => bounded_text(reader, limits).map(ContentBlock::Text),
        2 => media(reader, MediaKind::Image, limits, budget).map(ContentBlock::Image),
        3 => media(reader, MediaKind::Audio, limits, budget).map(ContentBlock::Audio),
        4 => media(reader, MediaKind::Document, limits, budget).map(ContentBlock::Document),
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
    budget: &mut DecodeBudget,
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
            let encoded = reader.read_bytes().map_err(codec)?;
            if encoded.len() > limits.max_inline_media_bytes() {
                return Err(invalid(
                    "inline_media",
                    "inline media exceeds its selected per-value allocation budget",
                ));
            }
            budget.charge_inline_media(encoded.len())?;
            MediaInput::inline(kind, media_type, owned_bytes(encoded, "inline_media")?, limits)
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
    let encoded = reader.read_bytes().map_err(codec)?;
    if encoded.len() > limits.max_extension_bytes() {
        return Err(invalid(
            "reasoning_replay",
            "reasoning replay exceeds its selected allocation budget",
        ));
    }
    let opaque = owned_bytes(encoded, "reasoning_replay")?;
    ReasoningReplay::new(summary, opaque, limits)
}

pub(super) fn extension(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ProviderExtension, ProtocolError> {
    let name = ExtensionName::new(reader.read_str().map_err(codec)?.to_owned())?;
    Ok(ProviderExtension::new(name, extension_json(reader, limits)?))
}

fn owned_bytes(bytes: &[u8], path: &'static str) -> Result<Vec<u8>, ProtocolError> {
    let mut owned = Vec::new();
    owned.try_reserve_exact(bytes.len()).map_err(|_| {
        invalid(path, "canonical value allocation is unavailable")
    })?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}
