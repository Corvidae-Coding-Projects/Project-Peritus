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
#[cfg(verus_only)]
pub use verified::{
    ancestor_number_precedes, append_only_length, authority_reflexive, authority_within_ceiling,
    dependency_precedes, digest_binding_reflexive, digest_is_bound, direct_predecessor_is_ancestor,
    earlier_dependency_is_legal, one_append_is_monotonic, protected_assets_unchanged,
    protected_invariance_reflexive, schema_is_compatible, singleton_schema_compatible,
};
pub use verified::{
    authority_is_non_widening, component_ids_are_unique, dependencies_are_resolved,
    graph_digest_is_bound, history_is_append_only, protected_assets_are_invariant,
    rollback_is_ancestor, topological_order_is_complete,
};

pub(crate) use canonical::{CanonicalEncoder, CanonicalReader};
