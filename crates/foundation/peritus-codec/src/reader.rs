//! Canonical primitive reader.

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "reader methods share CodecError semantics; fixed-array conversions follow exact checked reads"
)]

use crate::{CodecError, CodecErrorKind, CodecLimit, CodecLimits};

/// Borrowing canonical reader with explicit resource ceilings.
#[derive(Debug)]
pub struct CanonicalReader<'a> {
    input: &'a [u8],
    offset: usize,
    base_offset: usize,
    limits: CodecLimits,
    depth: usize,
}

impl<'a> CanonicalReader<'a> {
    /// Creates a reader over one payload under an explicit shared codec contract.
    #[must_use]
    pub const fn new(input: &'a [u8], limits: CodecLimits) -> Self {
        Self { input, offset: 0, base_offset: 0, limits, depth: 0 }
    }

    pub(crate) const fn with_base(
        input: &'a [u8],
        limits: CodecLimits,
        base_offset: usize,
    ) -> Self {
        Self { input, offset: 0, base_offset, limits, depth: 0 }
    }

    /// Returns the absolute current byte offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.base_offset + self.offset
    }

    /// Returns unread payload bytes.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.input.len() - self.offset
    }

    /// Requires complete consumption of the declared value.
    pub const fn finish(self) -> Result<(), CodecError> {
        if self.input.len() > self.limits.max_payload_bytes {
            return Err(CodecError::limited(self.base_offset, CodecLimit::PayloadBytes));
        }
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::TrailingBytes, self.offset()))
        }
    }

    /// Reads one byte.
    pub fn read_u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    /// Reads one big-endian `u16`.
    pub fn read_u16(&mut self) -> Result<u16, CodecError> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("exact checked length");
        Ok(u16::from_be_bytes(bytes))
    }

    /// Reads one big-endian `u32`.
    pub fn read_u32(&mut self) -> Result<u32, CodecError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("exact checked length");
        Ok(u32::from_be_bytes(bytes))
    }

    /// Reads one big-endian `u64`.
    pub fn read_u64(&mut self) -> Result<u64, CodecError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("exact checked length");
        Ok(u64::from_be_bytes(bytes))
    }

    /// Reads an exact fixed-width byte array.
    pub fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        Ok(self.take(N)?.try_into().expect("exact checked length"))
    }

    /// Reads a closed zero/one boolean tag.
    pub fn read_bool(&mut self) -> Result<bool, CodecError> {
        let offset = self.offset();
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CodecError::new(CodecErrorKind::InvalidBoolean, offset)),
        }
    }

    /// Reads a closed zero/one option-presence tag.
    pub fn read_option_tag(&mut self) -> Result<bool, CodecError> {
        let offset = self.offset();
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CodecError::new(CodecErrorKind::InvalidOption, offset)),
        }
    }

    /// Reads a length-prefixed borrowed opaque byte value.
    pub fn read_bytes(&mut self) -> Result<&'a [u8], CodecError> {
        let length_offset = self.offset();
        let length = usize::try_from(self.read_u32()?)
            .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, length_offset))?;
        if length > self.limits.max_opaque_bytes {
            return Err(CodecError::limited(length_offset, CodecLimit::OpaqueBytes));
        }
        self.take(length)
    }

    /// Reads a length-prefixed owned opaque byte value.
    pub fn read_bytes_owned(&mut self) -> Result<Vec<u8>, CodecError> {
        let offset = self.offset();
        let bytes = self.read_bytes()?;
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .map_err(|_| CodecError::new(CodecErrorKind::AllocationUnavailable, offset))?;
        owned.extend_from_slice(bytes);
        Ok(owned)
    }

    /// Reads a length-prefixed borrowed UTF-8 string.
    pub fn read_str(&mut self) -> Result<&'a str, CodecError> {
        let length_offset = self.offset();
        let length = usize::try_from(self.read_u32()?)
            .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, length_offset))?;
        if length > self.limits.max_string_bytes {
            return Err(CodecError::limited(length_offset, CodecLimit::StringBytes));
        }
        let bytes = self.take(length)?;
        core::str::from_utf8(bytes)
            .map_err(|_| CodecError::new(CodecErrorKind::InvalidUtf8, length_offset + 4))
    }

    /// Reads a collection count and checks its minimum encoded extent before allocation.
    ///
    /// `minimum_item_bytes` is a schema fact, not a work allowance. Use zero only for a
    /// stand-alone count or a schema whose elements consume no wire bytes. Owned collection
    /// decoders must supply their elements' positive minimum width before reserving storage.
    pub fn read_collection_len(&mut self, minimum_item_bytes: usize) -> Result<usize, CodecError> {
        let offset = self.offset();
        let value = usize::try_from(self.read_u32()?)
            .map_err(|_| CodecError::new(CodecErrorKind::LengthOverflow, offset))?;
        if value > self.limits.max_collection_items {
            Err(CodecError::limited(offset, CodecLimit::CollectionItems))
        } else {
            let minimum = value
                .checked_mul(minimum_item_bytes)
                .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, offset))?;
            if minimum > self.remaining() {
                return Err(CodecError::new(CodecErrorKind::Truncated, self.offset()));
            }
            Ok(value)
        }
    }

    /// Prepares owned collection storage after checking its count and encoded minimum extent.
    ///
    /// Use the count returned by [`Self::read_collection_len`]. Allocator pressure is a typed
    /// capacity failure, distinct from malformed declarations and caller-selected work limits.
    pub fn reserve_collection<T>(&self, count: usize) -> Result<Vec<T>, CodecError> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| CodecError::new(CodecErrorKind::AllocationUnavailable, self.offset()))?;
        Ok(values)
    }

    /// Executes one nested aggregate decode under the depth limit.
    pub fn nested<T>(
        &mut self,
        decode: impl FnOnce(&mut Self) -> Result<T, CodecError>,
    ) -> Result<T, CodecError> {
        if self.limits.max_nesting_depth != CodecLimits::UNLIMITED_NESTING
            && self.depth >= usize::from(self.limits.max_nesting_depth)
        {
            return Err(CodecError::limited(self.offset(), CodecLimit::NestingDepth));
        }
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, self.offset()))?;
        let result = decode(self);
        self.depth -= 1;
        result
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        if self.input.len() > self.limits.max_payload_bytes {
            return Err(CodecError::limited(self.base_offset, CodecLimit::PayloadBytes));
        }
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| CodecError::new(CodecErrorKind::LengthOverflow, self.offset()))?;
        if end > self.input.len() {
            return Err(CodecError::new(CodecErrorKind::Truncated, self.offset()));
        }
        let value = &self.input[self.offset..end];
        self.offset = end;
        Ok(value)
    }
}
