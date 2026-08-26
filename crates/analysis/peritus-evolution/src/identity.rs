//! F0-owned stable nominal identities.

use crate::{EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery};
use peritus_types::Sha256Digest;

/// Identifies one immutable harness change manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChangeManifestId([u8; 16]);

impl ChangeManifestId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one immutable isolated harness variant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VariantId([u8; 16]);

impl VariantId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one explicitly interacting change group.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InteractionGroupId([u8; 16]);

impl InteractionGroupId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one deterministic attribution result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AttributionId([u8; 16]);

impl AttributionId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one deterministic selection decision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SelectionId([u8; 16]);

impl SelectionId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one immutable production-promotion proposal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PromotionId([u8; 16]);

impl PromotionId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one append-only production rollback.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RollbackId([u8; 16]);

impl RollbackId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one production-pointer activation record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActivationId([u8; 16]);

impl ActivationId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvolutionError> {
        validate_identity(bytes).map(Self)
    }
    pub(crate) fn derive(domain: &[u8], digest: Sha256Digest) -> Self {
        Self(derive_identity(domain, digest))
    }
    /// Borrows the exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

fn validate_identity(bytes: [u8; 16]) -> Result<[u8; 16], EvolutionError> {
    if bytes == [0; 16] {
        Err(EvolutionError::new(
            EvolutionErrorKind::InvalidInput,
            EvolutionOperation::ValidateIdentity,
            EvolutionRecovery::CorrectInput,
            "identity is zero",
        ))
    } else {
        Ok(bytes)
    }
}

fn derive_identity(domain: &[u8], digest: Sha256Digest) -> [u8; 16] {
    let mut bytes = Vec::with_capacity(domain.len() + 32);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(digest.as_bytes());
    let derived = peritus_codec::sha256(&bytes);
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&derived.as_bytes()[..16]);
    if identity == [0; 16] {
        identity[15] = 1;
    }
    identity
}

pub(crate) fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(
        domain.len() + parts.iter().map(|part| part.len().saturating_add(8)).sum::<usize>(),
    );
    bytes.extend_from_slice(domain);
    for part in parts {
        bytes.extend_from_slice(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    peritus_codec::sha256(&bytes)
}

pub(crate) fn push_bytes(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    output.extend_from_slice(value);
}
