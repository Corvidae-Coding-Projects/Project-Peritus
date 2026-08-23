//! Typed canonical message traits.

#![allow(
    clippy::missing_errors_doc,
    reason = "typed message APIs forward the complete CodecError vocabulary declared by their reader and writer"
)]

use crate::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits};

/// Canonical payload encoder for one stable message family and schema version.
pub trait CanonicalEncode {
    /// Stable nonzero family tag.
    const FAMILY: u16;
    /// Stable nonzero schema version.
    const SCHEMA_VERSION: u16;

    /// Encodes only this message's payload.
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError>;
}

/// Canonical payload decoder for one stable message family and schema version.
pub trait CanonicalDecode: Sized {
    /// Stable nonzero family tag.
    const FAMILY: u16;
    /// Stable nonzero schema version.
    const SCHEMA_VERSION: u16;

    /// Decodes only this message's payload.
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError>;
}

/// Encodes one typed message into its complete canonical frame.
pub fn encode_message<T: CanonicalEncode>(
    message: &T,
    limits: CodecLimits,
) -> Result<Vec<u8>, CodecError> {
    let mut writer = CanonicalWriter::new(limits);
    message.encode_payload(&mut writer)?;
    crate::encode_frame(T::FAMILY, T::SCHEMA_VERSION, &writer.into_bytes(), limits)
}

/// Decodes one complete canonical frame as the requested typed message.
pub fn decode_message<T: CanonicalDecode>(
    input: &[u8],
    limits: CodecLimits,
) -> Result<T, CodecError> {
    let frame = crate::decode_frame(input, limits)?;
    if frame.header().family() != T::FAMILY {
        return Err(CodecError::new(CodecErrorKind::WrongFamily, 6));
    }
    if frame.header().schema_version() != T::SCHEMA_VERSION {
        return Err(CodecError::new(CodecErrorKind::WrongSchemaVersion, 8));
    }
    let mut reader = CanonicalReader::with_base(frame.payload(), limits, crate::HEADER_LEN);
    let message = T::decode_payload(&mut reader)?;
    reader.finish()?;
    Ok(message)
}
