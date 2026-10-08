//! Incremental exact canonical parity and digest verification without an output buffer.

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{CanonicalWrite, CodecError, CodecErrorKind};

/// Streaming exact-byte verifier with resumable byte progress and a matching SHA-256 transcript.
///
/// A caller may retain this value across cooperative interruptions. Successful writes advance only
/// across bytes that exactly match `expected`; no second canonical output buffer is constructed.
#[derive(Clone, Debug)]
pub struct CanonicalVerifier<'a> {
    expected: &'a [u8],
    offset: usize,
    digest: Sha256,
}

impl<'a> CanonicalVerifier<'a> {
    /// Begins exact verification with an unprefixed SHA-256 transcript.
    #[must_use]
    pub fn new(expected: &'a [u8]) -> Self {
        Self::with_digest_prefix(expected, &[])
    }

    /// Begins exact verification after adding a domain prefix to the SHA-256 transcript.
    #[must_use]
    pub fn with_digest_prefix(expected: &'a [u8], digest_prefix: &[u8]) -> Self {
        let mut digest = Sha256::new();
        digest.update(digest_prefix);
        Self { expected, offset: 0, digest }
    }

    /// Returns the exact number of bytes already matched and incorporated into the digest.
    #[must_use]
    pub const fn verified_bytes(&self) -> usize {
        self.offset
    }

    /// Returns whether the complete expected byte sequence has been matched.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.offset == self.expected.len()
    }

    /// Requires complete parity and returns the digest of the exact matched transcript.
    ///
    /// # Errors
    ///
    /// Returns a trailing-byte error if expected input remains unmatched.
    pub fn finish(self) -> Result<Sha256Digest, CodecError> {
        if self.offset != self.expected.len() {
            return Err(CodecError::at(CodecErrorKind::TrailingBytes, self.offset));
        }
        Ok(Sha256Digest::new(self.digest.finalize().into()))
    }

    fn accept(&mut self, value: &[u8]) -> Result<(), CodecError> {
        let end = self
            .offset
            .checked_add(value.len())
            .ok_or_else(|| CodecError::at(CodecErrorKind::LengthOverflow, self.offset))?;
        let Some(expected) = self.expected.get(self.offset..end) else {
            return Err(CodecError::at(CodecErrorKind::Truncated, self.expected.len()));
        };
        if let Some(relative) = expected.iter().zip(value).position(|(left, right)| left != right) {
            return Err(CodecError::at(
                CodecErrorKind::InvalidDomainValue,
                self.offset + relative,
            ));
        }
        self.digest.update(value);
        self.offset = end;
        Ok(())
    }
}

impl CanonicalWrite for CanonicalVerifier<'_> {
    fn write_fixed(&mut self, value: &[u8]) -> Result<(), CodecError> {
        self.accept(value)
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<(), CodecError> {
        let length = u32::try_from(value.len())
            .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, self.offset))?;
        self.write_u32(length)?;
        self.accept(value)
    }

    fn write_str(&mut self, value: &str) -> Result<(), CodecError> {
        self.write_bytes(value.as_bytes())
    }

    fn write_collection_len(&mut self, value: usize) -> Result<(), CodecError> {
        let length = u32::try_from(value)
            .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, self.offset))?;
        self.write_u32(length)
    }

    fn write_u8(&mut self, value: u8) -> Result<(), CodecError> {
        self.accept(&[value])
    }

    fn write_u16(&mut self, value: u16) -> Result<(), CodecError> {
        self.accept(&value.to_be_bytes())
    }

    fn write_u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.accept(&value.to_be_bytes())
    }

    fn write_u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.accept(&value.to_be_bytes())
    }

    fn write_bool(&mut self, value: bool) -> Result<(), CodecError> {
        self.write_u8(u8::from(value))
    }

    fn write_option_tag(&mut self, present: bool) -> Result<(), CodecError> {
        self.write_u8(u8::from(present))
    }
}
