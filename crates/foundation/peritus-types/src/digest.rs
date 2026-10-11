//! Exact digest bytes without hashing or authenticity semantics.

#[doc = " An exact 32-byte value intended to hold an already-computed SHA-256 digest."]
#[doc = ""]
#[doc = " This type checks only the representation size, which is fixed by its array input. It does not"]
#[doc = " hash content, validate provenance, or claim that the bytes authenticate any object."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest {
    bytes: [u8; 32],
}
impl Sha256Digest {
    #[doc = " The digest representation length in bytes."]
    pub const LENGTH: usize = 32;
    #[doc = " Stores an exact digest byte pattern, including the all-zero pattern."]
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    #[doc = " Borrows the exact stored bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
    #[doc = " Consumes the digest and returns the exact stored bytes."]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        self.bytes
    }
}
