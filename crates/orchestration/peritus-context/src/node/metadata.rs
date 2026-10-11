//! Exact context-node metadata admission and semantic cloning.

use super::{RequirementMode, RoleVisibility};
use crate::{
    AuthorityClass, ContentKind, ContextError, ContextErrorKind, ContextLimits, ContextNodeId,
    Provenance, TrustClass,
};
use peritus_role::ContextClass;

mod model;
mod validation;

#[doc = " Immutable metadata used by the checked context-node constructor."]
#[derive(Debug, Eq, PartialEq)]
pub struct ContextNodeMetadata {
    pub(super) id: ContextNodeId,
    pub(super) provenance: Provenance,
    pub(super) authority: AuthorityClass,
    pub(super) trust: TrustClass,
    pub(super) context_class: ContextClass,
    pub(super) content_kind: ContentKind,
    pub(super) token_estimate: u64,
    pub(super) recency_sequence: u64,
    pub(super) requirement: RequirementMode,
    pub(super) priority: u16,
    pub(super) visibility: RoleVisibility,
    pub(super) dependencies: Vec<ContextNodeId>,
}
impl ContextNodeMetadata {
    #[doc = " Validates all metadata compatibility and canonical dependency invariants."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first typed metadata or dependency error."]
    #[allow(
        clippy::too_many_arguments,
        reason = "all security metadata is explicit at the boundary"
    )]
    pub fn new(
        id: ContextNodeId,
        provenance: Provenance,
        authority: AuthorityClass,
        trust: TrustClass,
        context_class: ContextClass,
        content_kind: ContentKind,
        token_estimate: u64,
        recency_sequence: u64,
        requirement: RequirementMode,
        priority: u16,
        visibility: RoleVisibility,
        dependencies: Vec<ContextNodeId>,
        limits: ContextLimits,
    ) -> Result<Self, ContextError> {
        if token_estimate == 0 {
            return Err(ContextError::node(ContextErrorKind::ZeroTokenEstimate, id));
        }
        if recency_sequence == 0 {
            return Err(ContextError::node(ContextErrorKind::ZeroRecency, id));
        }
        if !provenance.permits_authority(authority) {
            return Err(ContextError::node(ContextErrorKind::IncompatibleAuthority, id));
        }
        if !provenance.permits_trust(trust) {
            return Err(ContextError::node(ContextErrorKind::IncompatibleTrust, id));
        }
        if !kind_matches_authority(content_kind, authority) {
            return Err(ContextError::node(ContextErrorKind::IncompatibleContentKind, id));
        }
        if dependencies.len() > limits.max_dependencies_per_node() {
            return Err(ContextError::node_numbers(
                ContextErrorKind::TooManyDependencies,
                id,
                limits.max_dependencies_per_node() as u64,
                dependencies.len() as u64,
            ));
        }
        validation::validate_dependencies(id, &dependencies)?;
        Ok(Self {
            id,
            provenance,
            authority,
            trust,
            context_class,
            content_kind,
            token_estimate,
            recency_sequence,
            requirement,
            priority,
            visibility,
            dependencies,
        })
    }
    #[doc = " Returns the stable node ID."]
    #[must_use]
    pub const fn id(&self) -> ContextNodeId {
        self.id
    }
    pub(super) const fn provenance_value(&self) -> Provenance {
        self.provenance
    }
    pub(super) const fn authority_value(&self) -> AuthorityClass {
        self.authority
    }
    pub(super) const fn trust_value(&self) -> TrustClass {
        self.trust
    }
    pub(super) const fn context_class_value(&self) -> ContextClass {
        self.context_class
    }
    pub(super) const fn content_kind_value(&self) -> ContentKind {
        self.content_kind
    }
    pub(super) const fn token_estimate_value(&self) -> u64 {
        self.token_estimate
    }
    pub(super) const fn recency_sequence_value(&self) -> u64 {
        self.recency_sequence
    }
    pub(super) const fn requirement_value(&self) -> RequirementMode {
        self.requirement
    }
    pub(super) const fn priority_value(&self) -> u16 {
        self.priority
    }
    pub(super) const fn visibility_value(&self) -> &RoleVisibility {
        &self.visibility
    }
    pub(super) const fn dependencies_value(&self) -> &[ContextNodeId] {
        self.dependencies.as_slice()
    }
    pub(crate) fn preserve_compaction_trust(self) -> Self {
        Self {
            id: self.id,
            provenance: self.provenance,
            authority: self.authority,
            trust: TrustClass::Trusted,
            context_class: self.context_class,
            content_kind: self.content_kind,
            token_estimate: self.token_estimate,
            recency_sequence: self.recency_sequence,
            requirement: self.requirement,
            priority: self.priority,
            visibility: self.visibility,
            dependencies: self.dependencies,
        }
    }
}
impl Clone for ContextNodeMetadata {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            provenance: self.provenance,
            authority: self.authority,
            trust: self.trust,
            context_class: self.context_class,
            content_kind: self.content_kind,
            token_estimate: self.token_estimate,
            recency_sequence: self.recency_sequence,
            requirement: self.requirement,
            priority: self.priority,
            visibility: self.visibility.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}
pub(super) const fn kind_matches_authority(kind: ContentKind, authority: AuthorityClass) -> bool {
    model::kind_matches_authority_exec(kind, authority)
}
