//! Deterministic SHA-256 helpers over retained evidence bytes.

use core::fmt::Write as _;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

/// Hashes exact in-memory bytes.
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

/// Renders a digest as exactly 64 lowercase hexadecimal digits.
#[must_use]
pub fn hex_digest(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.into_bytes() {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the private manifest module consumes this sibling encoding helper"
)]
pub(super) fn hex_identifier(bytes: &[u8; 16]) -> String {
    let mut output = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
