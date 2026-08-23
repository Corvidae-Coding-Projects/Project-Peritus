//! SHA-256 over exact canonical bytes.

#![allow(
    clippy::missing_errors_doc,
    reason = "canonical hashing forwards the complete typed CodecError vocabulary"
)]

use crate::{CanonicalEncode, CodecError, CodecLimits};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

/// Hashes exact bytes without assigning authenticity semantics.
#[must_use]
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

/// Encodes and hashes one complete canonical frame.
pub fn canonical_sha256<T: CanonicalEncode>(
    message: &T,
    limits: CodecLimits,
) -> Result<Sha256Digest, CodecError> {
    crate::encode_message(message, limits).map(|bytes| sha256(&bytes))
}
