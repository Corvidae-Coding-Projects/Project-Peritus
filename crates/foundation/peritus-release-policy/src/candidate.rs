//! Exact release-candidate, platform, toolchain, profile, and schema identities.

use crate::{CandidateId, ConstructionError, ConstructionErrorKind};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

mod git;
mod platform;
mod version;

pub use git::{GitCommitId, GitObjectFormat};
pub use platform::{Architecture, OperatingSystem, PlatformIdentity, PlatformMatrix};
pub use version::ReleaseVersion;

/// Exact Rust, Verus, vstd, and solver toolchain closure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::struct_field_names, reason = "each digest names a distinct toolchain component")]
pub struct ToolchainIdentity {
    rust_digest: Sha256Digest,
    verus_digest: Sha256Digest,
    vstd_digest: Sha256Digest,
    solver_digest: Sha256Digest,
}

impl ToolchainIdentity {
    /// Creates an exact non-placeholder toolchain identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroDigest`] when any component is a placeholder.
    pub fn new(
        rust_digest: Sha256Digest,
        verus_digest: Sha256Digest,
        vstd_digest: Sha256Digest,
        solver_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        require_digest(rust_digest)?;
        require_digest(verus_digest)?;
        require_digest(vstd_digest)?;
        require_digest(solver_digest)?;
        Ok(Self { rust_digest, verus_digest, vstd_digest, solver_digest })
    }

    /// Returns the exact Rust toolchain digest.
    #[must_use]
    pub const fn rust_digest(&self) -> Sha256Digest { self.rust_digest }

    /// Returns the exact Verus toolchain digest.
    #[must_use]
    pub const fn verus_digest(&self) -> Sha256Digest { self.verus_digest }

    /// Returns the exact vstd revision digest.
    #[must_use]
    pub const fn vstd_digest(&self) -> Sha256Digest { self.vstd_digest }

    /// Returns the exact solver identity digest.
    #[must_use]
    pub const fn solver_digest(&self) -> Sha256Digest { self.solver_digest }
}

/// Exact runtime/qualification profile revision and content.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProfileIdentity {
    revision: u64,
    digest: Sha256Digest,
}

impl ProfileIdentity {
    /// Creates a positive, content-bound profile identity.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero revision or digest.
    pub fn new(revision: u64, digest: Sha256Digest) -> Result<Self, ConstructionError> {
        require_revision(revision)?;
        require_digest(digest)?;
        Ok(Self { revision, digest })
    }

    /// Returns the profile revision.
    #[must_use]
    pub const fn revision(&self) -> u64 { self.revision }

    /// Returns the exact profile content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
}

/// Exact policy/evidence/report/artifact schema set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchemaIdentity {
    policy: u64,
    evidence: u64,
    report: u64,
    artifact: u64,
    catalog_digest: Sha256Digest,
}

impl SchemaIdentity {
    /// Creates a positive, content-bound schema identity.
    ///
    /// # Errors
    ///
    /// Returns a typed error for any zero revision or placeholder digest.
    pub fn new(
        policy: u64,
        evidence: u64,
        report: u64,
        artifact: u64,
        catalog_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        require_revision(policy)?;
        require_revision(evidence)?;
        require_revision(report)?;
        require_revision(artifact)?;
        require_digest(catalog_digest)?;
        Ok(Self { policy, evidence, report, artifact, catalog_digest })
    }

    /// Returns the release-policy schema revision.
    #[must_use]
    pub const fn policy(&self) -> u64 { self.policy }

    /// Returns the evidence schema revision.
    #[must_use]
    pub const fn evidence(&self) -> u64 { self.evidence }

    /// Returns the report schema revision.
    #[must_use]
    pub const fn report(&self) -> u64 { self.report }

    /// Returns the artifact schema revision.
    #[must_use]
    pub const fn artifact(&self) -> u64 { self.artifact }

    /// Returns the closed catalog digest.
    #[must_use]
    pub const fn catalog_digest(&self) -> Sha256Digest { self.catalog_digest }
}

/// Immutable identity to which every H4 observation must bind exactly.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseCandidate {
    id: CandidateId,
    commit: GitCommitId,
    version: ReleaseVersion,
    platforms: PlatformMatrix,
    toolchain: ToolchainIdentity,
    profile: ProfileIdentity,
    schemas: SchemaIdentity,
    source_revision: u64,
    manifest_digest: Sha256Digest,
}

impl ReleaseCandidate {
    /// Creates the complete exact release-candidate identity.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a zero source revision or placeholder manifest digest.
    #[allow(clippy::too_many_arguments, reason = "the release identity keeps every exact binding explicit")]
    pub fn new(
        id: CandidateId,
        commit: GitCommitId,
        version: ReleaseVersion,
        platforms: PlatformMatrix,
        toolchain: ToolchainIdentity,
        profile: ProfileIdentity,
        schemas: SchemaIdentity,
        source_revision: u64,
        manifest_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        require_revision(source_revision)?;
        require_digest(manifest_digest)?;
        Ok(Self {
            id,
            commit,
            version,
            platforms,
            toolchain,
            profile,
            schemas,
            source_revision,
            manifest_digest,
        })
    }

    /// Returns the nominal candidate identity.
    #[must_use]
    pub const fn id(&self) -> CandidateId { self.id }

    /// Returns the exact Git commit.
    #[must_use]
    pub const fn commit(&self) -> GitCommitId { self.commit }

    /// Returns the exact version.
    #[must_use]
    pub const fn version(&self) -> ReleaseVersion { self.version }

    /// Returns the exact tier-one platform matrix.
    #[must_use]
    pub const fn platforms(&self) -> PlatformMatrix { self.platforms }

    /// Returns the exact toolchain identity.
    #[must_use]
    pub const fn toolchain(&self) -> ToolchainIdentity { self.toolchain }

    /// Returns the exact runtime and qualification profile.
    #[must_use]
    pub const fn profile(&self) -> ProfileIdentity { self.profile }

    /// Returns the exact schema set.
    #[must_use]
    pub const fn schemas(&self) -> SchemaIdentity { self.schemas }

    /// Returns the exact producing source revision.
    #[must_use]
    pub const fn source_revision(&self) -> (source_revision: u64)
        ensures source_revision == self.spec_source_revision()
    {
        self.source_revision
    }

    /// Specification view of the exact producing source revision.
    pub closed spec fn spec_source_revision(&self) -> u64 { self.source_revision }

    /// Returns the digest of the canonical release manifest binding all candidate fields.
    #[must_use]
    pub const fn manifest_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_manifest_digest(),
    {
        self.manifest_digest
    }

    /// Specification view of the canonical complete-candidate manifest digest.
    pub closed spec fn spec_manifest_digest(&self) -> Sha256Digest { self.manifest_digest }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evidence bindings compare canonical candidate-manifest digests"
)]
pub(crate) const fn digests_equal(
    left: Sha256Digest,
    right: Sha256Digest,
) -> (equal: bool)
    ensures equal == digest_bytes_equal_from(left.spec_bytes(), right.spec_bytes(), 0),
{
    digest_bytes_equal_exec(*left.as_bytes(), *right.as_bytes(), 0)
}

/// Mathematical suffix equality for canonical SHA-256 candidate bindings.
pub open spec fn digest_bytes_equal_from(
    left: [u8; 32],
    right: [u8; 32],
    index: nat,
) -> bool
    decreases 32 - index,
{
    if index >= 32 {
        true
    } else {
        left[index as int] == right[index as int]
            && digest_bytes_equal_from(left, right, index + 1)
    }
}

const fn digest_bytes_equal_exec(
    left: [u8; 32],
    right: [u8; 32],
    index: usize,
) -> (equal: bool)
    requires index <= 32,
    ensures equal == digest_bytes_equal_from(left, right, index as nat),
    decreases 32 - index,
{
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        digest_bytes_equal_exec(left, right, index + 1)
    }
}

#[allow(clippy::redundant_pub_crate, reason = "checked constructors in sibling modules share digest validation")]
pub(crate) const fn digest_nonzero(digest: Sha256Digest) -> bool {
    digest_bytes_nonzero(digest.as_bytes())
}

const fn digest_bytes_nonzero(bytes: &[u8; 32]) -> bool {
    let mut index = 0;
    while index < bytes.len()
        invariant 0 <= index <= bytes.len(),
        decreases bytes.len() - index,
    {
        if bytes[index] != 0 { return true; }
        index += 1;
    }
    false
}

#[allow(clippy::redundant_pub_crate, reason = "checked constructors in sibling modules share digest validation")]
pub(crate) const fn require_digest(digest: Sha256Digest) -> Result<(), ConstructionError> {
    if digest_nonzero(digest) {
        Ok(())
    } else {
        Err(ConstructionError::new(ConstructionErrorKind::ZeroDigest))
    }
}

const fn require_revision(revision: u64) -> Result<(), ConstructionError> {
    if revision == 0 {
        Err(ConstructionError::new(ConstructionErrorKind::ZeroRevision))
    } else {
        Ok(())
    }
}

} // verus!
