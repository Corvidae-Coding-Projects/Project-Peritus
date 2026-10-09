//! Common bounded view over canonical versioned envelope bytes.

use crate::{ProtocolError, ProtocolErrorKind};

const HEADER_BYTES: usize = 8;

/// Borrowed, structurally checked canonical envelope header and payload.
///
/// Individual envelope types own their semantic validation. This common view provides bounded
/// framing and exact byte preservation for durable transport, fixtures, and digest verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalEnvelope<'a> {
    bytes: &'a [u8],
    family: u16,
    version: u16,
}

impl<'a> CanonicalEnvelope<'a> {
    /// Parses one canonical envelope within an explicit transport byte ceiling.
    ///
    /// # Errors
    ///
    /// Rejects zero/exceeded bounds, a truncated header, a foreign magic value, family zero, or
    /// an unsupported protocol version.
    pub fn parse(bytes: &'a [u8], maximum_bytes: usize) -> Result<Self, ProtocolError> {
        if maximum_bytes == 0 || bytes.len() > maximum_bytes || bytes.len() < HEADER_BYTES {
            return Err(invalid("canonical envelope is truncated or exceeds its transport bound"));
        }
        if &bytes[..4] != b"PTL1" {
            return Err(invalid("canonical envelope magic is invalid"));
        }
        let family = u16::from_be_bytes([bytes[4], bytes[5]]);
        let version = u16::from_be_bytes([bytes[6], bytes[7]]);
        if family == 0 || version != 1 {
            return Err(invalid("canonical envelope family or protocol version is unsupported"));
        }
        Ok(Self { bytes, family, version })
    }

    /// Returns the stable envelope-family tag.
    #[must_use]
    pub const fn family(self) -> u16 {
        self.family
    }

    /// Returns the canonical protocol version.
    #[must_use]
    pub const fn version(self) -> u16 {
        self.version
    }

    /// Borrows the family-specific payload after the common header.
    #[must_use]
    pub fn payload(self) -> &'a [u8] {
        &self.bytes[HEADER_BYTES..]
    }

    /// Borrows the exact input bytes for lossless framing round trips.
    #[must_use]
    pub const fn canonical_bytes(self) -> &'a [u8] {
        self.bytes
    }
}

fn invalid(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEnvelope, "canonical_envelope", detail)
}
