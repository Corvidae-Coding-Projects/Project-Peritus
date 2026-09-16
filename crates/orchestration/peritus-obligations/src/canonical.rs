//! Canonical digest preimage for one exact requirement ledger.

pub mod encode;
#[cfg(verus_only)]
pub mod model;

#[cfg(not(verus_only))]
use peritus_types::Sha256Digest;
#[cfg(not(verus_only))]
use sha2::{Digest, Sha256};

/// Hashes exact canonical bytes without assigning authenticity semantics.
#[cfg(not(verus_only))]
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}
