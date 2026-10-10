//! Exact release-candidate, platform, toolchain, profile, and schema identities.

use crate::CandidateId;
use peritus_types::Sha256Digest;

mod construction;
pub mod equality;
mod git;
mod platform;
mod version;
pub use equality::candidate_matches;
pub use git::{GitCommitId, GitObjectFormat};
pub use platform::{Architecture, OperatingSystem, PlatformIdentity, PlatformMatrix};
pub use version::ReleaseVersion;
#[doc = " Exact Rust, Verus, vstd, and solver toolchain closure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::struct_field_names, reason = "each digest names a distinct toolchain component")]
pub struct ToolchainIdentity {
    rust_digest: Sha256Digest,
    verus_digest: Sha256Digest,
    vstd_digest: Sha256Digest,
    solver_digest: Sha256Digest,
}
impl ToolchainIdentity {
    #[doc = " Returns the exact Rust toolchain digest."]
    #[must_use]
    pub const fn rust_digest(&self) -> Sha256Digest {
        self.rust_digest
    }
    #[doc = " Returns the exact Verus toolchain digest."]
    #[must_use]
    pub const fn verus_digest(&self) -> Sha256Digest {
        self.verus_digest
    }
    #[doc = " Returns the exact vstd revision digest."]
    #[must_use]
    pub const fn vstd_digest(&self) -> Sha256Digest {
        self.vstd_digest
    }
    #[doc = " Returns the exact solver identity digest."]
    #[must_use]
    pub const fn solver_digest(&self) -> Sha256Digest {
        self.solver_digest
    }
}
#[doc = " Exact runtime/qualification profile revision and content."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProfileIdentity {
    revision: u64,
    digest: Sha256Digest,
}
impl ProfileIdentity {
    #[doc = " Returns the profile revision."]
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[doc = " Returns the exact profile content digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}
#[doc = " Exact policy/evidence/report/artifact schema set."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchemaIdentity {
    policy: u64,
    evidence: u64,
    report: u64,
    artifact: u64,
    catalog_digest: Sha256Digest,
}
impl SchemaIdentity {
    #[doc = " Returns the release-policy schema revision."]
    #[must_use]
    pub const fn policy(&self) -> u64 {
        self.policy
    }
    #[doc = " Returns the evidence schema revision."]
    #[must_use]
    pub const fn evidence(&self) -> u64 {
        self.evidence
    }
    #[doc = " Returns the report schema revision."]
    #[must_use]
    pub const fn report(&self) -> u64 {
        self.report
    }
    #[doc = " Returns the artifact schema revision."]
    #[must_use]
    pub const fn artifact(&self) -> u64 {
        self.artifact
    }
    #[doc = " Returns the closed catalog digest."]
    #[must_use]
    pub const fn catalog_digest(&self) -> Sha256Digest {
        self.catalog_digest
    }
}
#[doc = " Immutable identity to which every H4 observation must bind exactly."]
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
    #[doc = " Returns the nominal candidate identity."]
    #[must_use]
    pub const fn id(&self) -> CandidateId {
        self.id
    }
    #[doc = " Returns the exact Git commit."]
    #[must_use]
    pub const fn commit(&self) -> GitCommitId {
        self.commit
    }
    #[doc = " Returns the exact version."]
    #[must_use]
    pub const fn version(&self) -> ReleaseVersion {
        self.version
    }
    #[doc = " Returns the exact tier-one platform matrix."]
    #[must_use]
    pub const fn platforms(&self) -> PlatformMatrix {
        self.platforms
    }
    #[doc = " Returns the exact toolchain identity."]
    #[must_use]
    pub const fn toolchain(&self) -> ToolchainIdentity {
        self.toolchain
    }
    #[doc = " Returns the exact runtime and qualification profile."]
    #[must_use]
    pub const fn profile(&self) -> ProfileIdentity {
        self.profile
    }
    #[doc = " Returns the exact schema set."]
    #[must_use]
    pub const fn schemas(&self) -> SchemaIdentity {
        self.schemas
    }
    #[doc = " Returns the exact producing source revision."]
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    #[doc = " Returns the digest of the canonical release manifest binding all candidate fields."]
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
}
