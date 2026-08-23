//! Artifact digest values and canonical lowercase encoding.

use peritus_types::Sha256Digest;

use crate::{ArtifactStoreError, ErrorCode, RecoveryClass};

/// Content identity for an artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactDigest(Sha256Digest);

impl ArtifactDigest {
    /// The lowercase hexadecimal representation length.
    pub const HEX_LENGTH: usize = 64;

    /// Wraps exact SHA-256 bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(Sha256Digest::new(bytes))
    }

    /// Wraps an existing primitive SHA-256 value.
    #[must_use]
    pub const fn from_sha256(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the primitive digest.
    #[must_use]
    pub const fn sha256(self) -> Sha256Digest {
        self.0
    }

    /// Returns exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Encodes the digest as canonical lowercase hexadecimal.
    #[must_use]
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(Self::HEX_LENGTH);
        for byte in self.as_bytes() {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }

    pub(crate) fn parse_internal_hex(value: &str) -> Result<Self, ArtifactStoreError> {
        if value.len() != Self::HEX_LENGTH {
            return Err(invalid_internal_digest());
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = decode_nibble(pair[0])?
                .checked_mul(16)
                .and_then(|high| high.checked_add(decode_nibble(pair[1]).ok()?))
                .ok_or_else(invalid_internal_digest)?;
        }
        Ok(Self::new(bytes))
    }
}

const fn decode_nibble(value: u8) -> Result<u8, ArtifactStoreError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(invalid_internal_digest()),
    }
}

const fn invalid_internal_digest() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::CorruptObject,
        RecoveryClass::TerminalIntegrity,
        "noncanonical digest in the internal store layout",
    )
}

impl From<Sha256Digest> for ArtifactDigest {
    fn from(value: Sha256Digest) -> Self {
        Self::from_sha256(value)
    }
}

impl From<ArtifactDigest> for Sha256Digest {
    fn from(value: ArtifactDigest) -> Self {
        value.sha256()
    }
}
