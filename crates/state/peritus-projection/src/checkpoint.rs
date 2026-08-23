//! Versioned projection identities, schemas, and journal-bound checkpoints.

use crate::{ProjectionError, ProjectionErrorKind, RecoveryClass};
use peritus_codec::sha256;
use peritus_types::Sha256Digest;
use std::{fmt, num::NonZeroU64};

/// Maximum UTF-8 bytes in a stable projection name.
pub const MAX_PROJECTION_NAME_BYTES: usize = 64;
/// Maximum bytes in a projection schema descriptor.
pub const MAX_SCHEMA_DESCRIPTOR_BYTES: usize = 16 * 1024;

/// Validated stable kebab-case projection name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectionName(String);

impl ProjectionName {
    /// Validates and owns a stable lowercase ASCII name.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, non-kebab-case, or leading/trailing-hyphen names.
    pub fn new(value: impl Into<String>) -> Result<Self, ProjectionError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_PROJECTION_NAME_BYTES
            && bytes.first() != Some(&b'-')
            && bytes.last() != Some(&b'-')
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
            && !bytes.windows(2).any(|pair| pair == b"--");
        if valid {
            Ok(Self(value))
        } else {
            Err(ProjectionError::new(
                ProjectionErrorKind::InvalidInput,
                RecoveryClass::CorrectInput,
                "validate projection name",
                "name must be bounded lowercase ASCII kebab-case",
            ))
        }
    }

    /// Borrows the stable name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Positive stable projection implementation version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectionVersion(NonZeroU64);

impl ProjectionVersion {
    /// Creates a nonzero version.
    #[must_use]
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Returns the positive version number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Stable name and version pair.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectionIdentity {
    name: ProjectionName,
    version: ProjectionVersion,
}

impl ProjectionIdentity {
    /// Creates an identity from validated parts.
    #[must_use]
    pub const fn new(name: ProjectionName, version: ProjectionVersion) -> Self {
        Self { name, version }
    }

    /// Returns the stable projection name.
    #[must_use]
    pub const fn name(&self) -> &ProjectionName {
        &self.name
    }

    /// Returns the implementation version.
    #[must_use]
    pub const fn version(&self) -> ProjectionVersion {
        self.version
    }
}

/// Immutable projection schema identity derived from an explicit descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionSchema {
    identity: ProjectionIdentity,
    digest: Sha256Digest,
}

impl ProjectionSchema {
    /// Hashes a bounded, nonempty canonical schema descriptor.
    ///
    /// # Errors
    ///
    /// Rejects an empty or oversized descriptor.
    pub fn new(identity: ProjectionIdentity, descriptor: &[u8]) -> Result<Self, ProjectionError> {
        if descriptor.is_empty() || descriptor.len() > MAX_SCHEMA_DESCRIPTOR_BYTES {
            return Err(ProjectionError::new(
                ProjectionErrorKind::InvalidInput,
                RecoveryClass::CorrectInput,
                "validate projection schema",
                "schema descriptor must be nonempty and bounded",
            ));
        }
        let mut bytes = Vec::with_capacity(identity.name().as_str().len() + descriptor.len() + 16);
        bytes.extend_from_slice(b"peritus-projection-schema-v1\0");
        bytes.extend_from_slice(identity.name().as_str().as_bytes());
        bytes.extend_from_slice(&identity.version().get().to_be_bytes());
        bytes.extend_from_slice(descriptor);
        Ok(Self { identity, digest: sha256(&bytes) })
    }

    /// Returns the name and implementation version.
    #[must_use]
    pub const fn identity(&self) -> &ProjectionIdentity {
        &self.identity
    }

    /// Returns the schema digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(crate) const fn from_digest(identity: ProjectionIdentity, digest: Sha256Digest) -> Self {
        Self { identity, digest }
    }
}

/// Durable binding between one projection payload and an exact checked journal head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    schema: ProjectionSchema,
    last_position: u64,
    journal_head_digest: Sha256Digest,
    payload_digest: Sha256Digest,
}

impl Checkpoint {
    /// Creates a checkpoint over already encoded deterministic payload bytes.
    #[must_use]
    pub fn new(
        schema: ProjectionSchema,
        last_position: u64,
        journal_head_digest: Sha256Digest,
        payload: &[u8],
    ) -> Self {
        Self { schema, last_position, journal_head_digest, payload_digest: sha256(payload) }
    }

    pub(crate) const fn from_digests(
        schema: ProjectionSchema,
        last_position: u64,
        journal_head_digest: Sha256Digest,
        payload_digest: Sha256Digest,
    ) -> Self {
        Self { schema, last_position, journal_head_digest, payload_digest }
    }

    /// Returns the projection schema.
    #[must_use]
    pub const fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }

    /// Returns the exact final global position.
    #[must_use]
    pub const fn last_position(&self) -> u64 {
        self.last_position
    }

    /// Returns the checked journal-head digest.
    #[must_use]
    pub const fn journal_head_digest(&self) -> Sha256Digest {
        self.journal_head_digest
    }

    /// Returns the deterministic payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }

    /// Checks that bytes still match the stored payload digest.
    #[must_use]
    pub fn binds_payload(&self, payload: &[u8]) -> bool {
        sha256(payload) == self.payload_digest
    }
}
