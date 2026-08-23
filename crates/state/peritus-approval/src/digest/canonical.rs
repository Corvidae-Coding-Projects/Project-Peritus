//! Verified bounded record framing and fixed-width big-endian encoding.

use vstd::prelude::*;

verus! {

#[allow(
    clippy::cast_possible_truncation,
    reason = "each cast selects one explicitly shifted byte of the fixed-width integer"
)]
pub(super) const fn be_u16(value: u16) -> [u8; 2] {
    [(value >> 8) as u8, value as u8]
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "each cast selects one explicitly shifted byte of the fixed-width integer"
)]
pub(super) const fn be_u32(value: u32) -> [u8; 4] {
    [
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ]
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "each cast selects one explicitly shifted byte of the fixed-width integer"
)]
pub(super) const fn be_u64(value: u64) -> [u8; 8] {
    [
        (value >> 56) as u8,
        (value >> 48) as u8,
        (value >> 40) as u8,
        (value >> 32) as u8,
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ]
}

/// Checked canonical field-record builder shared by the three cryptographic boundaries.
pub struct CanonicalEncoder {
    bytes: Vec<u8>,
    maximum: usize,
}

impl CanonicalEncoder {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the immediately preceding guard rejects every domain length above u16::MAX"
    )]
    pub fn record(domain: &[u8], maximum: usize) -> Result<Self, crate::ApprovalError> {
        let mut encoder = Self { bytes: Vec::new(), maximum };
        encoder.raw(b"PERITUS\0B1\0APPROVAL\0V1\0")?;
        if domain.len() > u16::MAX as usize {
            return Err(crate::ApprovalError::PreimageTooLarge);
        }
        let length = domain.len() as u16;
        encoder.raw(&be_u16(length))?;
        encoder.raw(domain)?;
        Ok(encoder)
    }

    pub fn field(&mut self, tag: u16, value: &[u8]) -> Result<(), crate::ApprovalError> {
        self.raw(&be_u16(tag))?;
        if value.len() > self.maximum {
            return Err(crate::ApprovalError::PreimageTooLarge);
        }
        let length = value.len() as u64;
        self.raw(&be_u64(length))?;
        self.raw(value)
    }

    pub fn finish(self) -> Vec<u8> { self.bytes }

    fn raw(&mut self, value: &[u8]) -> Result<(), crate::ApprovalError> {
        let next = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or(crate::ApprovalError::PreimageTooLarge)?;
        if next > self.maximum {
            return Err(crate::ApprovalError::PreimageTooLarge);
        }
        self.bytes.extend_from_slice(value);
        Ok(())
    }
}

} // verus!
