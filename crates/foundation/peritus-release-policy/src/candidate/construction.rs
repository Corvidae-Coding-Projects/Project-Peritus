//! Exact checked construction of complete release-candidate components.

use super::{
    GitCommitId, PlatformMatrix, ProfileIdentity, ReleaseCandidate, ReleaseVersion, SchemaIdentity,
    ToolchainIdentity,
};
use crate::{CandidateId, ConstructionError};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

impl ToolchainIdentity {
    /// Exact admission predicate for a complete toolchain identity.
    pub open spec fn inputs_valid(
        rust_digest: Sha256Digest,
        verus_digest: Sha256Digest,
        vstd_digest: Sha256Digest,
        solver_digest: Sha256Digest,
    ) -> bool {
        crate::validation::spec_digest_nonzero(rust_digest)
            && crate::validation::spec_digest_nonzero(verus_digest)
            && crate::validation::spec_digest_nonzero(vstd_digest)
            && crate::validation::spec_digest_nonzero(solver_digest)
    }

    /// Creates an exact non-placeholder toolchain identity.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ConstructionErrorKind::ZeroDigest`] when any component is a placeholder.
    pub fn new(
        rust_digest: Sha256Digest,
        verus_digest: Sha256Digest,
        vstd_digest: Sha256Digest,
        solver_digest: Sha256Digest,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(
                rust_digest, verus_digest, vstd_digest, solver_digest),
            match result {
                Ok(value) => value.spec_rust_digest() == rust_digest
                    && value.spec_verus_digest() == verus_digest
                    && value.spec_vstd_digest() == vstd_digest
                    && value.spec_solver_digest() == solver_digest,
                Err(error) => error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest,
            },
    {
        crate::validation::require_digest(rust_digest)?;
        crate::validation::require_digest(verus_digest)?;
        crate::validation::require_digest(vstd_digest)?;
        crate::validation::require_digest(solver_digest)?;
        Ok(Self { rust_digest, verus_digest, vstd_digest, solver_digest })
    }
}

impl ProfileIdentity {
    /// Exact admission predicate for a profile identity.
    pub open spec fn inputs_valid(revision: u64, digest: Sha256Digest) -> bool {
        revision > 0 && crate::validation::spec_digest_nonzero(digest)
    }

    /// Exact first-error result for rejected profile inputs.
    pub open spec fn construction_error(
        revision: u64,
        digest: Sha256Digest,
        error: ConstructionError,
    ) -> bool {
        if revision == 0 {
            error.spec_kind() == crate::ConstructionErrorKind::ZeroRevision
        } else {
            !crate::validation::spec_digest_nonzero(digest)
                && error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest
        }
    }

    /// Creates a positive, content-bound profile identity.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero revision or digest.
    pub fn new(
        revision: u64,
        digest: Sha256Digest,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(revision, digest),
            match result {
                Ok(value) => value.spec_revision() == revision
                    && value.spec_digest() == digest,
                Err(error) => Self::construction_error(revision, digest, error),
            },
    {
        crate::validation::require_revision(revision)?;
        crate::validation::require_digest(digest)?;
        Ok(Self { revision, digest })
    }
}

impl SchemaIdentity {
    /// Exact admission predicate for a complete schema identity.
    pub open spec fn inputs_valid(
        policy: u64,
        evidence: u64,
        report: u64,
        artifact: u64,
        catalog_digest: Sha256Digest,
    ) -> bool {
        policy > 0 && evidence > 0 && report > 0 && artifact > 0
            && crate::validation::spec_digest_nonzero(catalog_digest)
    }

    /// Exact observable first-error result for rejected schema inputs.
    pub open spec fn construction_error(
        policy: u64,
        evidence: u64,
        report: u64,
        artifact: u64,
        catalog_digest: Sha256Digest,
        error: ConstructionError,
    ) -> bool {
        if policy == 0 || evidence == 0 || report == 0 || artifact == 0 {
            error.spec_kind() == crate::ConstructionErrorKind::ZeroRevision
        } else {
            !crate::validation::spec_digest_nonzero(catalog_digest)
                && error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest
        }
    }

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
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(
                policy, evidence, report, artifact, catalog_digest),
            match result {
                Ok(value) => value.spec_policy() == policy
                    && value.spec_evidence() == evidence
                    && value.spec_report() == report
                    && value.spec_artifact() == artifact
                    && value.spec_catalog_digest() == catalog_digest,
                Err(error) => Self::construction_error(
                    policy, evidence, report, artifact, catalog_digest, error),
            },
    {
        crate::validation::require_revision(policy)?;
        crate::validation::require_revision(evidence)?;
        crate::validation::require_revision(report)?;
        crate::validation::require_revision(artifact)?;
        crate::validation::require_digest(catalog_digest)?;
        Ok(Self { policy, evidence, report, artifact, catalog_digest })
    }
}

impl ReleaseCandidate {
    /// Exact admission predicate for the complete candidate container.
    pub open spec fn inputs_valid(source_revision: u64, manifest_digest: Sha256Digest) -> bool {
        source_revision > 0 && crate::validation::spec_digest_nonzero(manifest_digest)
    }

    /// Exact first-error result for rejected candidate-container inputs.
    pub open spec fn construction_error(
        source_revision: u64,
        manifest_digest: Sha256Digest,
        error: ConstructionError,
    ) -> bool {
        if source_revision == 0 {
            error.spec_kind() == crate::ConstructionErrorKind::ZeroRevision
        } else {
            !crate::validation::spec_digest_nonzero(manifest_digest)
                && error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest
        }
    }

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
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(source_revision, manifest_digest),
            match result {
                Ok(value) => value.spec_id() == id
                    && value.spec_commit() == commit
                    && value.spec_version() == version
                    && value.spec_platforms() == platforms
                    && value.spec_toolchain() == toolchain
                    && value.spec_profile() == profile
                    && value.spec_schemas() == schemas
                    && value.spec_source_revision() == source_revision
                    && value.spec_manifest_digest() == manifest_digest,
                Err(error) => Self::construction_error(
                    source_revision, manifest_digest, error),
            },
    {
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

} // verus!
