//! Strong harness-domain identities and digest roles.

use peritus_types::Sha256Digest;

use crate::domain::{HarnessDomainError, HarnessDomainErrorKind};

const MAX_COMPONENT_ID_BYTES: usize = 128;

/// Stable, lineage-local component identity independent of manifest position.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComponentId(String);

impl ComponentId {
    /// Validates a nonempty portable component identity.
    ///
    /// # Errors
    ///
    /// Rejects identifiers longer than 128 bytes or containing characters other than ASCII
    /// letters, digits, `.`, `_`, and `-`. The first character must be alphanumeric.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::EmptyValue,
                "component identifier is empty",
            ));
        }
        if value.len() > MAX_COMPONENT_ID_BYTES {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::ValueTooLong,
                "component identifier exceeds 128 bytes",
            ));
        }
        let mut characters = value.bytes();
        let first = characters
            .next()
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::EmptyValue))?;
        if !first.is_ascii_alphanumeric()
            || !characters
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidValue,
                "component identifier is not portable canonical ASCII",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the canonical UTF-8 identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the identity and returns its canonical string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for ComponentId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Digest of the exact committed harness manifest bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManifestDigest(Sha256Digest);

impl ManifestDigest {
    /// Wraps an exact caller-computed SHA-256 digest without adding authenticity claims.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the exact digest value.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Digest of a complete checked harness graph.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GraphDigest(Sha256Digest);

impl GraphDigest {
    /// Wraps an exact caller-computed SHA-256 digest without adding authenticity claims.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the exact digest value.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Digest of one immutable complete harness revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RevisionDigest(Sha256Digest);

impl RevisionDigest {
    /// Wraps an exact caller-computed SHA-256 digest without adding authenticity claims.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the exact digest value.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Digest identifying an executable or stored component artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactDigest(Sha256Digest);

impl ArtifactDigest {
    /// Wraps an exact caller-computed SHA-256 digest without adding authenticity claims.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the exact digest value.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Caller-chosen entropy binding one genesis lineage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineageSeed(Sha256Digest);

impl LineageSeed {
    /// Wraps an exact caller-computed SHA-256 digest without adding authenticity claims.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the exact digest value.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl From<Sha256Digest> for ArtifactDigest {
    fn from(value: Sha256Digest) -> Self {
        Self::new(value)
    }
}

impl From<ArtifactDigest> for Sha256Digest {
    fn from(value: ArtifactDigest) -> Self {
        value.digest()
    }
}
