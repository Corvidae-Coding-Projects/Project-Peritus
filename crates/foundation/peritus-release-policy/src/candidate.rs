//! Exact release-candidate, platform, toolchain, profile, and schema identities.

use crate::CandidateId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

mod construction;
mod git;
pub mod equality;
mod platform;
mod version;

pub use equality::candidate_matches;
#[cfg(verus_only)]
pub use equality::{candidate_matches_exactly, digest_matches};

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
    /// Returns the exact Rust toolchain digest.
    #[must_use]
    pub const fn rust_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_rust_digest()
    {
        self.rust_digest
    }

    /// Returns the exact Verus toolchain digest.
    #[must_use]
    pub const fn verus_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_verus_digest()
    {
        self.verus_digest
    }

    /// Returns the exact vstd revision digest.
    #[must_use]
    pub const fn vstd_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_vstd_digest()
    {
        self.vstd_digest
    }

    /// Returns the exact solver identity digest.
    #[must_use]
    pub const fn solver_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_solver_digest()
    {
        self.solver_digest
    }

    /// Logical view of the exact Rust toolchain digest.
    pub closed spec fn spec_rust_digest(&self) -> Sha256Digest { self.rust_digest }

    /// Logical view of the exact Verus toolchain digest.
    pub closed spec fn spec_verus_digest(&self) -> Sha256Digest { self.verus_digest }

    /// Logical view of the exact vstd revision digest.
    pub closed spec fn spec_vstd_digest(&self) -> Sha256Digest { self.vstd_digest }

    /// Logical view of the exact solver identity digest.
    pub closed spec fn spec_solver_digest(&self) -> Sha256Digest { self.solver_digest }
}

/// Exact runtime/qualification profile revision and content.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProfileIdentity {
    revision: u64,
    digest: Sha256Digest,
}

impl ProfileIdentity {
    /// Returns the profile revision.
    #[must_use]
    pub const fn revision(&self) -> (revision: u64) ensures revision == self.spec_revision() {
        self.revision
    }

    /// Returns the exact profile content digest.
    #[must_use]
    pub const fn digest(&self) -> (digest: Sha256Digest) ensures digest == self.spec_digest() {
        self.digest
    }

    /// Logical view of the profile revision.
    pub closed spec fn spec_revision(&self) -> u64 { self.revision }

    /// Logical view of the exact profile content digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.digest }
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
    /// Returns the release-policy schema revision.
    #[must_use]
    pub const fn policy(&self) -> (revision: u64) ensures revision == self.spec_policy() {
        self.policy
    }

    /// Returns the evidence schema revision.
    #[must_use]
    pub const fn evidence(&self) -> (revision: u64) ensures revision == self.spec_evidence() {
        self.evidence
    }

    /// Returns the report schema revision.
    #[must_use]
    pub const fn report(&self) -> (revision: u64) ensures revision == self.spec_report() {
        self.report
    }

    /// Returns the artifact schema revision.
    #[must_use]
    pub const fn artifact(&self) -> (revision: u64) ensures revision == self.spec_artifact() {
        self.artifact
    }

    /// Returns the closed catalog digest.
    #[must_use]
    pub const fn catalog_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_catalog_digest()
    {
        self.catalog_digest
    }

    /// Logical view of the release-policy schema revision.
    pub closed spec fn spec_policy(&self) -> u64 { self.policy }

    /// Logical view of the evidence schema revision.
    pub closed spec fn spec_evidence(&self) -> u64 { self.evidence }

    /// Logical view of the report schema revision.
    pub closed spec fn spec_report(&self) -> u64 { self.report }

    /// Logical view of the artifact schema revision.
    pub closed spec fn spec_artifact(&self) -> u64 { self.artifact }

    /// Logical view of the closed catalog digest.
    pub closed spec fn spec_catalog_digest(&self) -> Sha256Digest { self.catalog_digest }
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
    /// Returns the nominal candidate identity.
    #[must_use]
    pub const fn id(&self) -> (id: CandidateId) ensures id == self.spec_id() { self.id }

    /// Returns the exact Git commit.
    #[must_use]
    pub const fn commit(&self) -> (commit: GitCommitId) ensures commit == self.spec_commit() {
        self.commit
    }

    /// Returns the exact version.
    #[must_use]
    pub const fn version(&self) -> (version: ReleaseVersion)
        ensures version == self.spec_version()
    {
        self.version
    }

    /// Returns the exact tier-one platform matrix.
    #[must_use]
    pub const fn platforms(&self) -> (platforms: PlatformMatrix)
        ensures platforms == self.spec_platforms()
    {
        self.platforms
    }

    /// Returns the exact toolchain identity.
    #[must_use]
    pub const fn toolchain(&self) -> (toolchain: ToolchainIdentity)
        ensures toolchain == self.spec_toolchain()
    {
        self.toolchain
    }

    /// Returns the exact runtime and qualification profile.
    #[must_use]
    pub const fn profile(&self) -> (profile: ProfileIdentity)
        ensures profile == self.spec_profile()
    {
        self.profile
    }

    /// Returns the exact schema set.
    #[must_use]
    pub const fn schemas(&self) -> (schemas: SchemaIdentity)
        ensures schemas == self.spec_schemas()
    {
        self.schemas
    }

    /// Logical view of the nominal candidate identity.
    pub closed spec fn spec_id(&self) -> CandidateId { self.id }

    /// Logical view of the exact Git commit.
    pub closed spec fn spec_commit(&self) -> GitCommitId { self.commit }

    /// Logical view of the exact release version.
    pub closed spec fn spec_version(&self) -> ReleaseVersion { self.version }

    /// Logical view of the exact tier-one platform matrix.
    pub closed spec fn spec_platforms(&self) -> PlatformMatrix { self.platforms }

    /// Logical view of the exact toolchain identity.
    pub closed spec fn spec_toolchain(&self) -> ToolchainIdentity { self.toolchain }

    /// Logical view of the exact runtime and qualification profile.
    pub closed spec fn spec_profile(&self) -> ProfileIdentity { self.profile }

    /// Logical view of the exact schema set.
    pub closed spec fn spec_schemas(&self) -> SchemaIdentity { self.schemas }

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

} // verus!
