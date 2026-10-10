//! Canonical digest preimage for one exact requirement ledger.

pub mod encode;

use peritus_types::Sha256Digest;

use sha2::{Digest, Sha256};

/// Hashes exact canonical bytes without assigning authenticity semantics.

pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}
