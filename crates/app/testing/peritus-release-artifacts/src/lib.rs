//! Deterministic contracts for H4 release artifacts and supply-chain evidence.
//!
//! This crate validates and reduces caller-observed bytes. It never creates signing keys, signs,
//! tags, publishes, or grants release authority.

mod artifact;
mod binding;
mod documentation;
mod error;
mod identity;
pub mod prelude;
mod provenance;
mod reproducibility;
mod sbom;
mod signature;

pub use artifact::{
    ArtifactEntry, ArtifactInventory, ArtifactRole, MAX_ARTIFACTS, MediaType, ReleasePath,
};
pub use binding::{CandidateCommit, PlatformTriple, ReleaseBinding, ReleaseVersion, ToolchainId};
pub use documentation::{
    DocumentationEvidence, DocumentationInventory, DocumentationKind, LicenseNotice,
    LicenseNoticeDocument,
};
pub use error::{ArtifactError, ArtifactErrorCode};
pub use identity::{BoundedId, Sha256Digest, digest_bytes};
pub use provenance::{
    BuildMaterial, ProvenanceStatement, ProvenanceTimestamps, SLSA_PROVENANCE_PREDICATE_TYPE,
};
pub use reproducibility::{
    BuildWitness, ReproducibilityComparison, ReproducibilityDifference,
    ReproducibilityDifferenceKind, compare_builds,
};
pub use sbom::{SpdxComponent, SpdxDocument, SpdxTimestamp};
pub use signature::{
    Ed25519PublicKey, Ed25519Signature, VerifiedSignature, verify_detached_ed25519,
};
