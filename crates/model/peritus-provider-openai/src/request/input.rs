//! Ordered message, content, media, tool-result, and replay admission.

use peritus_model_protocol::{ContentBlock, MediaKind, ModelRequest};
use peritus_provider_core::ProviderCoreError;

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
                    if media.reference_for_wire().is_some()
                        || !matches!(media.media_type().as_str(), "audio/wav" | "audio/mpeg") =>
                {
                    return Err(error::invalid(
                        "OpenAI audio input requires inline or artifact-backed WAV or MPEG bytes",
                    ));
                }
                ContentBlock::Reasoning(replay)
                    if core::str::from_utf8(replay.opaque_for_wire()).is_err() =>
                {
                    return Err(error::invalid(
                        "OpenAI reasoning replay must be a wire string",
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

pub(super) fn validate_resolved(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    for message in request.messages() {
        for block in message.content() {
            if let ContentBlock::Image(media)
            | ContentBlock::Audio(media)
            | ContentBlock::Document(media) = block
                && media.inline_bytes_for_wire().is_none()
                && media.reference_for_wire().is_none()
            {
                return Err(error::invalid(
                    "Peritus artifact media must be resolved at the OpenAI encoding boundary",
                ));
            }
        }
    }
    Ok(())
}
