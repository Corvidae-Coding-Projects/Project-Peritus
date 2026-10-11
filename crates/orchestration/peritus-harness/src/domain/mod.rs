//! Pure checked harness graph, immutable revision, and append-only history domain.

mod authority;
mod canonical;
mod catalog;
mod compatibility;
mod component;
mod component_canonical;
mod content;
mod error;
mod graph;
mod graph_validation;
mod history;
mod identity;
mod limits;
mod revision;
mod value;
mod verified;

pub use authority::{Authority, AuthoritySet};
pub use catalog::{ComponentKind, ProtectionClass};
pub use compatibility::{
    CompatibilityContract, DependencyRequirement, FeatureTag, SchemaInterval, SchemaVersion,
};
pub use component::{
    ArtifactRoot, ComponentDeclaration, ComponentIdentity, ComponentIntegrity, ComponentLocation,
    ComponentOwnership, ComponentRequirements,
};
pub use content::{ComponentContents, VerifiedComponentContent};
pub use error::{HarnessDomainError, HarnessDomainErrorKind, HarnessLimitKind};
pub use graph::{CheckedHarnessGraph, GraphEnvironment, ProtectedAsset, ResolvedEdge};
pub use history::{HarnessHistory, RollbackSelection};
pub use identity::{
    ArtifactDigest, ComponentId, GraphDigest, LineageSeed, ManifestDigest, RevisionDigest,
};
pub use limits::HarnessLimits;
pub use revision::{HarnessRevision, HarnessRevisionIdentity};
pub use value::{MediaType, Owner, Provenance, SourcePath, TargetPath};

pub use verified::{
    authority_is_non_widening, component_ids_are_unique, dependencies_are_resolved,
    graph_digest_is_bound, history_is_append_only, protected_assets_are_invariant,
    rollback_is_ancestor, topological_order_is_complete,
};

pub(crate) use canonical::{CanonicalEncoder, CanonicalReader};
