//! Validated protocol identities and bounded text.

use crate::{ProtocolError, ProtocolErrorKind};
use peritus_types::Sha256Digest;

const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_IDENTITY_BYTES: usize = 256;
const MAX_KEY_BYTES: usize = 128;

/// UTF-8 diagnostic/rendering text with a protocol-wide byte bound.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BoundedText(String);

impl BoundedText {
    /// Validates nonempty bounded text.
    ///
    /// # Errors
    ///
    /// Rejects empty text, NUL, or more than 16 KiB.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        validate_text(&value, MAX_TEXT_BYTES, "text")?;
        Ok(Self(value))
    }

    /// Borrows the validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Exact implementation/catalog identity reported by a dispatcher.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ImplementationIdentity(String);

impl ImplementationIdentity {
    /// Creates a bounded stable identity.
    ///
    /// # Errors
    ///
    /// Rejects empty, control-containing, or oversized identities.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        validate_identity(&value, MAX_IDENTITY_BYTES, "implementation_identity")?;
        Ok(Self(value))
    }

    /// Borrows the stable identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Caller-supplied bounded idempotency identity.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Creates an idempotency identity.
    ///
    /// # Errors
    ///
    /// Rejects empty, control-containing, or oversized keys.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        validate_identity(&value, MAX_KEY_BYTES, "idempotency_key")?;
        Ok(Self(value))
    }

    /// Borrows the stable key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Three-component semantic tool version.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl SemanticVersion {
    /// Creates a version with a nonzero major component.
    ///
    /// # Errors
    ///
    /// Version zero is reserved for invalid/unversioned values.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Result<Self, ProtocolError> {
        if major == 0 {
            Err(ProtocolError::new(
                ProtocolErrorKind::InvalidVersion,
                String::new(),
                "semantic major version must be nonzero",
            ))
        } else {
            Ok(Self { major, minor, patch })
        }
    }

    /// Returns the major component.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }
    /// Returns the minor component.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
    /// Returns the patch component.
    #[must_use]
    pub const fn patch(self) -> u16 {
        self.patch
    }
}

/// Domain-specific wrapper for a canonical schema or descriptor digest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaDigest(Sha256Digest);

impl SchemaDigest {
    /// Wraps a SHA-256 digest computed over canonical protocol bytes.
    #[must_use]
    pub const fn new(value: Sha256Digest) -> Self {
        Self(value)
    }
    /// Returns the underlying digest value.
    #[must_use]
    pub const fn get(self) -> Sha256Digest {
        self.0
    }
    /// Returns the digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

fn validate_text(value: &str, max: usize, path: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.len() > max || value.contains('\0') {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidText,
            path,
            "text is empty, contains NUL, or exceeds its byte bound",
        ));
    }
    Ok(())
}

fn validate_identity(value: &str, max: usize, path: &str) -> Result<(), ProtocolError> {
    validate_text(value, max, path)?;
    if value.chars().any(char::is_control) {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidText,
            path,
            "identity contains a control character",
        ));
    }
    Ok(())
}
