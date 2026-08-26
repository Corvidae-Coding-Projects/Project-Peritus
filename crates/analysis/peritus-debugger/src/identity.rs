//! Nominal E2 identities and content-address derivation.

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

/// Identifies one durable E2 debugger job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DebuggerJobId([u8; 16]);

impl DebuggerJobId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one immutable evidence-selection manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SelectionManifestId([u8; 16]);

impl SelectionManifestId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one immutable debugger report.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReportId([u8; 16]);

impl ReportId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one immutable report claim.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClaimId([u8; 16]);

impl ClaimId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one immutable root-cause candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CauseId([u8; 16]);

impl CauseId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one immutable cross-run diagnostic pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternId([u8; 16]);

impl PatternId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one optional model-analysis plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModelAnalysisId([u8; 16]);

impl ModelAnalysisId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

/// Identifies one exact cross-slice analysis subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubjectId([u8; 16]);

impl SubjectId {
    /// Creates a nonzero identity.
    ///
    /// # Errors
    /// Returns an invalid-input error for the reserved all-zero representation.
    pub fn new(bytes: [u8; 16]) -> Result<Self, DebuggerError> {
        checked_identity(bytes).map(Self)
    }
    /// Borrows the exact binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Returns the exact binary representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
    /// Derives a domain-separated identity from canonical bytes.
    ///
    /// # Errors
    /// Rejects an empty/non-NUL-terminated domain or all-zero projection.
    pub fn derive(domain: &[u8], canonical: &[u8]) -> Result<Self, DebuggerError> {
        derive_identity(domain, canonical).map(Self)
    }
}

fn checked_identity(bytes: [u8; 16]) -> Result<[u8; 16], DebuggerError> {
    if nonzero(bytes) { Ok(bytes) } else { Err(invalid_identity()) }
}

fn derive_identity(domain: &[u8], canonical: &[u8]) -> Result<[u8; 16], DebuggerError> {
    if domain.is_empty() || domain.last() != Some(&0) {
        return Err(DebuggerError::new(
            DebuggerErrorKind::InvalidInput,
            DebuggerOperation::ValidateBinding,
            DebuggerRecovery::CorrectInput,
            "identity domain must be nonempty and NUL terminated",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(canonical);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    checked_identity(bytes)
}

/// Computes a domain-separated SHA-256 digest over canonical E2 bytes.
#[must_use]
#[allow(
    clippy::redundant_pub_crate,
    reason = "domain hashing is shared by sibling private modules without becoming public API"
)]
pub(crate) fn domain_digest(domain: &[u8], canonical: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(canonical);
    Sha256Digest::new(hasher.finalize().into())
}

const fn nonzero(bytes: [u8; 16]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0 {
            return true;
        }
        index += 1;
    }
    false
}

fn invalid_identity() -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::InvalidInput,
        DebuggerOperation::ValidateBinding,
        DebuggerRecovery::CorrectInput,
        "all-zero debugger identity is reserved",
    )
}
