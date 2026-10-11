//! Exact checked construction of complete release-candidate components.

use super::{
    GitCommitId, PlatformMatrix, ProfileIdentity, ReleaseCandidate, ReleaseVersion, SchemaIdentity,
    ToolchainIdentity,
};
use crate::{CandidateId, ConstructionError};
use peritus_types::Sha256Digest;

impl ToolchainIdentity {
    #[doc = " Creates an exact non-placeholder toolchain identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`crate::ConstructionErrorKind::ZeroDigest`] when any component is a placeholder."]
    pub fn new(
        rust_digest: Sha256Digest,
        verus_digest: Sha256Digest,
        vstd_digest: Sha256Digest,
        solver_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(rust_digest)?;
        crate::validation::require_digest(verus_digest)?;
        crate::validation::require_digest(vstd_digest)?;
        crate::validation::require_digest(solver_digest)?;
        Ok(Self { rust_digest, verus_digest, vstd_digest, solver_digest })
    }
}
impl ProfileIdentity {
    #[doc = " Creates a positive, content-bound profile identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for zero revision or digest."]
    pub fn new(revision: u64, digest: Sha256Digest) -> Result<Self, ConstructionError> {
        crate::validation::require_revision(revision)?;
        crate::validation::require_digest(digest)?;
        Ok(Self { revision, digest })
    }
}
impl SchemaIdentity {
    #[doc = " Creates a positive, content-bound schema identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for any zero revision or placeholder digest."]
    pub fn new(
        policy: u64,
        evidence: u64,
        report: u64,
        artifact: u64,
        catalog_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_revision(policy)?;
        crate::validation::require_revision(evidence)?;
        crate::validation::require_revision(report)?;
        crate::validation::require_revision(artifact)?;
        crate::validation::require_digest(catalog_digest)?;
        Ok(Self { policy, evidence, report, artifact, catalog_digest })
    }
}
impl ReleaseCandidate {
    #[doc = " Creates the complete exact release-candidate identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for a zero source revision or placeholder manifest digest."]
    #[allow(
        clippy::too_many_arguments,
        reason = "the release identity keeps every exact binding explicit"
    )]
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
        crate::validation::require_revision(source_revision)?;
        crate::validation::require_digest(manifest_digest)?;
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
}
