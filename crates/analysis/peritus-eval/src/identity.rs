//! E3-owned validated identities and digest roles.

use peritus_types::Sha256Digest;

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation, invalid};

/// Identifies one immutable dataset lineage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DatasetId([u8; 16]);

impl DatasetId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvaluationError> {
        validate_identity(bytes).map(Self)
    }

    /// Borrows exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one dataset-local task.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId([u8; 16]);

impl TaskId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvaluationError> {
        validate_identity(bytes).map(Self)
    }

    /// Borrows exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one complete deterministic evaluation plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvaluationPlanId([u8; 16]);

impl EvaluationPlanId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvaluationError> {
        validate_identity(bytes).map(Self)
    }

    /// Borrows exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one logical task/arm/ordinal rollout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RolloutId([u8; 16]);

impl RolloutId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvaluationError> {
        validate_identity(bytes).map(Self)
    }

    /// Borrows exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one immutable evaluation report.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvaluationReportId([u8; 16]);

impl EvaluationReportId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvaluationError> {
        validate_identity(bytes).map(Self)
    }

    /// Borrows exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one versioned metric definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MetricId([u8; 16]);

impl MetricId {
    /// Creates a nonzero stable identity.
    ///
    /// # Errors
    /// Rejects the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, EvaluationError> {
        validate_identity(bytes).map(Self)
    }

    /// Borrows exact canonical bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Digest of one complete checked dataset manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DatasetDigest(Sha256Digest);

impl DatasetDigest {
    /// Wraps an exact caller-computed SHA-256 digest.
    #[must_use]
    pub const fn new(value: Sha256Digest) -> Self {
        Self(value)
    }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Digest of one complete frozen evaluation profile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProfileDigest(Sha256Digest);

impl ProfileDigest {
    /// Wraps an exact caller-computed SHA-256 digest.
    #[must_use]
    pub const fn new(value: Sha256Digest) -> Self {
        Self(value)
    }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Digest of one complete deterministic evaluation plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlanDigest(Sha256Digest);

impl PlanDigest {
    /// Wraps an exact caller-computed SHA-256 digest.
    #[must_use]
    pub const fn new(value: Sha256Digest) -> Self {
        Self(value)
    }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Digest of one canonical result or report root.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResultDigest(Sha256Digest);

impl ResultDigest {
    /// Wraps an exact caller-computed SHA-256 digest.
    #[must_use]
    pub const fn new(value: Sha256Digest) -> Self {
        Self(value)
    }

    /// Returns the exact digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }

    /// Borrows exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

fn validate_identity(bytes: [u8; 16]) -> Result<[u8; 16], EvaluationError> {
    if nonzero(bytes) {
        Ok(bytes)
    } else {
        Err(invalid(
            EvaluationErrorKind::Manifest,
            EvaluationOperation::ValidateDataset,
            "all-zero evaluation identity is reserved",
        ))
    }
}

fn nonzero(bytes: [u8; 16]) -> bool {
    bytes != [0; 16]
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "stable identity derivation is shared by sibling implementation modules"
)]
pub(crate) fn derived_identity(domain: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut bytes = Vec::with_capacity(
        domain.len() + parts.iter().map(|part| part.len().saturating_add(4)).sum::<usize>(),
    );
    bytes.extend_from_slice(domain);
    for part in parts {
        bytes.extend_from_slice(&u32::try_from(part.len()).unwrap_or(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    let digest = peritus_codec::sha256(&bytes);
    let mut result = [0_u8; 16];
    result.copy_from_slice(&digest.as_bytes()[..16]);
    result[0] |= 0x40;
    result
}
