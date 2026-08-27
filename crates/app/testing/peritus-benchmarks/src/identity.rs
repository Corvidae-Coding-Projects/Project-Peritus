//! Portable identifiers, digests, and evidence-relative paths.

use std::fmt;
use std::path::{Component, Path};
use std::str::FromStr;

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::QualificationError;

const MAX_IDENTIFIER_BYTES: usize = 96;

/// A portable stable identifier used in datasets and evidence bindings.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct StableId(String);

impl StableId {
    /// Validates an identifier containing ASCII letters, digits, `.`, `_`, or `-`.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the identifier is empty, exceeds 96 bytes, or contains
    /// a character outside the portable grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty() {
            return Err(invalid_id(value, "identifier must not be empty"));
        }
        if value.len() > MAX_IDENTIFIER_BYTES {
            return Err(invalid_id(value, "identifier exceeds 96 bytes"));
        }
        if !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)) {
            return Err(invalid_id(value, "identifier contains a non-portable character"));
        }
        Ok(Self(value))
    }

    /// Returns the validated identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for StableId {
    type Err = QualificationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

const fn invalid_id(value: String, reason: &'static str) -> QualificationError {
    QualificationError::InvalidIdentifier { value, reason }
}

/// A lowercase hexadecimal SHA-256 digest.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Hashes bytes into a portable lowercase hexadecimal digest.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(encoded)
    }

    /// Validates already-computed lowercase hexadecimal digest text.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] unless the value is exactly 64 lowercase hexadecimal bytes.
    pub fn parse(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.len() != 64
            || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(QualificationError::InvalidDigest(value));
        }
        Ok(Self(value))
    }

    /// Returns lowercase hexadecimal digest text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Sha256Digest {
    type Err = QualificationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// A normalized path confined below an evidence bundle root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ArtifactPath(String);

impl ArtifactPath {
    /// Validates a forward-slash relative path without parent components.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the path is empty, absolute, contains backslashes, or
    /// contains a non-normal component such as `..`.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty() || value.contains('\\') {
            return Err(QualificationError::InvalidArtifactPath(value));
        }
        let path = Path::new(&value);
        let valid = !path.is_absolute()
            && path.components().all(|component| matches!(component, Component::Normal(_)));
        if !valid {
            return Err(QualificationError::InvalidArtifactPath(value));
        }
        Ok(Self(value))
    }

    /// Returns normalized relative path text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactPath, Sha256Digest, StableId};

    #[test]
    fn stable_id_rejects_whitespace() {
        assert!(StableId::new("not portable").is_err());
    }

    #[test]
    fn artifact_path_rejects_escape() {
        assert!(ArtifactPath::new("../outside.json").is_err());
    }

    #[test]
    fn digest_round_trips() {
        let digest = Sha256Digest::of_bytes(b"peritus");
        assert_eq!(Sha256Digest::parse(digest.as_str()).expect("digest"), digest);
    }
}
