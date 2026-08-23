//! Canonical primitive writer.

#![allow(
    clippy::missing_errors_doc,
    reason = "writer methods share the explicit CodecError limit and arithmetic vocabulary"
)]

use crate::{CodecError, CodecErrorKind, CodecLimit, CodecLimits};

/// Transactional primitive writer bounded by [`CodecLimits`].
#[derive(Debug)]
pub struct CanonicalWriter {
    bytes: Vec<u8>,
    limits: CodecLimits,
    depth: u16,
}

impl CanonicalWriter {
    /// Creates an empty bounded payload writer.
    #[must_use]
    pub const fn new(limits: CodecLimits) -> Self {
        Self { bytes: Vec::new(), limits, depth: 0 }
    }

    /// Returns the number of bytes written.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether no bytes have been written.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Borrows the current canonical prefix.
    #[must_use]
    pub const fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Finishes the payload.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Writes one exact byte.
    pub fn write_u8(&mut self, value: u8) -> Result<(), CodecError> {
        self.write_fixed(&[value])
    }

    /// Writes one big-endian `u16`.
    pub fn write_u16(&mut self, value: u16) -> Result<(), CodecError> {
        self.write_fixed(&value.to_be_bytes())
    }

    /// Writes one big-endian `u32`.
    pub fn write_u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.write_fixed(&value.to_be_bytes())
    }

    /// Writes one big-endian `u64`.
    pub fn write_u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.write_fixed(&value.to_be_bytes())
    }

    /// Writes a closed zero/one boolean tag.
    pub fn write_bool(&mut self, value: bool) -> Result<(), CodecError> {
        self.write_u8(u8::from(value))
    }

    /// Writes a closed zero/one option-presence tag.
    pub fn write_option_tag(&mut self, present: bool) -> Result<(), CodecError> {
        self.write_u8(u8::from(present))
    }

    /// Writes fixed-width bytes without a length prefix.
    pub fn write_fixed(&mut self, value: &[u8]) -> Result<(), CodecError> {
        self.reserve_payload(value.len())?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    /// Writes a length-prefixed opaque byte value.
    pub fn write_bytes(&mut self, value: &[u8]) -> Result<(), CodecError> {
        if value.len() > self.limits.max_opaque_bytes {
            return Err(CodecError::limited(self.len(), CodecLimit::OpaqueBytes));
        }
        let length = u32::try_from(value.len())
            .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, self.len()))?;
        let additional = 4usize
            .checked_add(value.len())
            .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, self.len()))?;
        self.reserve_payload(additional)?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    /// Writes a length-prefixed UTF-8 string.
    pub fn write_str(&mut self, value: &str) -> Result<(), CodecError> {
        if value.len() > self.limits.max_string_bytes {
            return Err(CodecError::limited(self.len(), CodecLimit::StringBytes));
        }
        let length = u32::try_from(value.len())
            .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, self.len()))?;
        let additional = 4usize
            .checked_add(value.len())
            .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, self.len()))?;
        self.reserve_payload(additional)?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    /// Writes a bounded collection count.
    pub fn write_collection_len(&mut self, value: usize) -> Result<(), CodecError> {
        if value > self.limits.max_collection_items {
            return Err(CodecError::limited(self.len(), CodecLimit::CollectionItems));
        }
        let value = u32::try_from(value)
            .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, self.len()))?;
        self.write_u32(value)
    }

    /// Executes one nested aggregate encoding under the depth limit.
    pub fn nested<T>(
        &mut self,
        encode: impl FnOnce(&mut Self) -> Result<T, CodecError>,
    ) -> Result<T, CodecError> {
        if self.depth >= self.limits.max_nesting_depth {
            return Err(CodecError::limited(self.len(), CodecLimit::NestingDepth));
        }
        self.depth += 1;
        let result = encode(self);
        self.depth -= 1;
        result
    }

    fn reserve_payload(&mut self, additional: usize) -> Result<(), CodecError> {
        let next = self
            .bytes
            .len()
            .checked_add(additional)
            .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, self.len()))?;
        if next > self.limits.max_payload_bytes {
            return Err(CodecError::limited(self.len(), CodecLimit::PayloadBytes));
        }
        self.bytes.reserve(additional);
        Ok(())
    }
}
