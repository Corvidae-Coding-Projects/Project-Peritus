//! Profile-independent canonical message archives using the existing C5 content encoding.

use crate::{Message, ProtocolError, ProtocolErrorKind, ProtocolLimits};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAGIC: [u8; 4] = *b"P5MS";
const VERSION: u16 = 1;

/// Encodes an ordered message sequence without a provider profile or continuation token.
///
/// This is storage, not authorization to replay its roles or provider-specific content. Hosts
/// must project archived messages through current policy and the receiving protocol.
///
/// # Errors
/// Rejects excessive messages or bytes and invalid bounded content encodings.
pub fn encode_messages(
    messages: &[Message],
    limits: ProtocolLimits,
) -> Result<Vec<u8>, ProtocolError> {
    if messages.len() > limits.max_messages() {
        return Err(invalid());
    }
    let mut writer = CanonicalWriter::new(codec_limits(limits));
    writer.write_fixed(&MAGIC).map_err(codec)?;
    writer.write_u16(VERSION).map_err(codec)?;
    writer.write_collection_len(messages.len()).map_err(codec)?;
    for message in messages {
        crate::canonical::message_value(&mut writer, message)?;
    }
    Ok(writer.into_bytes())
}

/// Decodes a version-one message archive without requiring the original provider profile.
///
/// # Errors
/// Rejects unknown versions, malformed/noncanonical data, invalid content, bounds, or trailing
/// bytes. Tool association and role visibility remain the assembling host's responsibility.
pub fn decode_messages(
    bytes: &[u8],
    limits: ProtocolLimits,
) -> Result<Vec<Message>, ProtocolError> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut reader = CanonicalReader::new(bytes, codec_limits(limits));
    if reader.read_fixed::<4>().map_err(codec)? != MAGIC
        || reader.read_u16().map_err(codec)? != VERSION
    {
        return Err(invalid());
    }
    let messages = crate::canonical_decode::decode_messages(&mut reader, limits)?;
    reader.finish().map_err(codec)?;
    if encode_messages(&messages, limits)? != bytes {
        return Err(invalid());
    }
    Ok(messages)
}

fn codec_limits(limits: ProtocolLimits) -> CodecLimits {
    CodecLimits::new(
        MAX_BYTES,
        MAX_BYTES,
        limits.max_messages().max(limits.max_content_blocks()).max(128),
        limits.max_text_bytes().max(8 * 1024),
        limits
            .max_inline_media_bytes()
            .max(limits.max_tool_argument_bytes())
            .max(limits.max_extension_bytes())
            .max(limits.max_schema_bytes()),
        128,
    )
}
fn codec(_: peritus_codec::CodecError) -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidContent,
        "message_archive",
        "canonical message bytes are malformed or incomplete",
    )
}
fn invalid() -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidContent,
        "message_archive",
        "invalid version, bounds, or canonical representation",
    )
}
