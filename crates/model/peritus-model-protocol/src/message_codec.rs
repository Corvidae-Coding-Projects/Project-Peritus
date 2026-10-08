//! Profile-independent canonical message archives using the existing C5 content encoding.

use crate::{Message, ProtocolError, ProtocolErrorKind, ProtocolLimits};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

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
    crate::request::validation::validate_messages(messages, limits, false)?;
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
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
    let mut reader = CanonicalReader::new(bytes, decoder_limits(bytes.len()));
    if reader.read_fixed::<4>().map_err(codec)? != MAGIC
        || reader.read_u16().map_err(codec)? != VERSION
    {
        return Err(invalid());
    }
    let messages = crate::canonical_decode::decode_messages(&mut reader, limits)?;
    crate::request::validation::validate_messages(&messages, limits, false)?;
    reader.finish().map_err(codec)?;
    if encode_messages(&messages, limits)? != bytes {
        return Err(invalid());
    }
    Ok(messages)
}

const fn decoder_limits(encoded_bytes: usize) -> CodecLimits {
    CodecLimits::new(
        encoded_bytes,
        encoded_bytes,
        usize::MAX,
        encoded_bytes,
        encoded_bytes,
        CodecLimits::UNLIMITED_NESTING,
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
