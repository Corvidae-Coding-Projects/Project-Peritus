//! Exact digest bytes without hashing or authenticity semantics.

use vstd::prelude::*;

verus! {

/// An exact 32-byte value intended to hold an already-computed SHA-256 digest.
///
/// This type checks only the representation size, which is fixed by its array input. It does not
/// hash content, validate provenance, or claim that the bytes authenticate any object.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest {
    bytes: [u8; 32],
}

impl Sha256Digest {
    /// The digest representation length in bytes.
    pub const LENGTH: usize = 32;

    /// Stores an exact digest byte pattern, including the all-zero pattern.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> (digest: Self)
        ensures
            digest.spec_bytes() == bytes,
    {
        Self { bytes }
    }

    /// Returns the mathematical byte-array view used by specifications.
    pub closed spec fn spec_bytes(&self) -> [u8; 32] {
        self.bytes
    }

    /// Borrows the exact stored bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 32])
        ensures
            *bytes == self.spec_bytes(),
    {
        &self.bytes
    }

    /// Consumes the digest and returns the exact stored bytes.
    #[must_use]
    pub const fn into_bytes(self) -> (bytes: [u8; 32])
        ensures
            bytes == self.spec_bytes(),
    {
        self.bytes
    }
}

} // verus!
