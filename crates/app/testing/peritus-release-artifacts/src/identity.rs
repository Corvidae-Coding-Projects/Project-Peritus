//! Validated identifiers and SHA-256 content identities.

use std::fmt;

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::{ArtifactError, ArtifactErrorCode};

const MAX_ID_BYTES: usize = 128;

/// A bounded portable identifier used for builders, components, and invocations.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BoundedId(String);

impl BoundedId {
    /// Validates a portable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when the value is empty, oversized, starts with punctuation, or
    /// contains bytes outside ASCII alphanumeric, `.`, `_`, `-`, `:`, `/`, or `@`.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        let mut bytes = value.bytes();
        let first_valid = bytes.next().is_some_and(|byte| byte.is_ascii_alphanumeric());
        let tail_valid = bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'@')
        });
        if value.len() > MAX_ID_BYTES || !first_valid || !tail_valid {
            return Err(ArtifactError::new(
                ArtifactErrorCode::InvalidValue,
                "validate identifier",
                "identifier violates the 1 through 128 byte portable grammar",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the validated identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BoundedId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A lowercase hexadecimal SHA-256 digest.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Creates a digest from exact digest bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parses exactly 64 lowercase hexadecimal bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for any noncanonical representation.
    pub fn parse(value: &str) -> Result<Self, ArtifactError> {
        if value.len() != 64
            || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ArtifactError::new(
                ArtifactErrorCode::InvalidValue,
                "parse SHA-256",
                "digest must be exactly 64 lowercase hexadecimal bytes",
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
        }
        Ok(Self(bytes))
    }

    /// Returns exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Encodes the digest as lowercase hexadecimal.
    #[must_use]
    pub fn to_hex(self) -> String {
        encode_hex(&self.0)
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Sha256Digest").field(&self.to_hex()).finish()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

/// Hashes exact bytes with SHA-256.
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

#[must_use]
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

const fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{Sha256Digest, digest_bytes};

    #[test]
    fn digest_round_trips_through_canonical_hex() {
        let digest = digest_bytes(b"peritus release");
        assert_eq!(Sha256Digest::parse(&digest.to_hex()).expect("canonical digest"), digest);
    }

    #[test]
    fn uppercase_digest_is_rejected() {
        assert!(Sha256Digest::parse(&"A".repeat(64)).is_err());
    }
}
