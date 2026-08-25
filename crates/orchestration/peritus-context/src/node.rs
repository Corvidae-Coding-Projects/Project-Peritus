//! Checked context nodes and canonical role/dependency metadata.

use crate::{
    AuthorityClass, ContentKind, ContextContent, ContextError, ContextErrorKind, ContextLimits,
    ContextNodeId, Provenance, TrustClass,
};
use peritus_policy::ActorRole;
use peritus_role::ContextClass;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Whether a node is a required root, a preferred dependency, or optional.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequirementMode {
    /// The node and its complete closure must be selected.
    Required,
    /// The node is required whenever a selected root depends on it.
    DependencyRequired,
    /// The node is eligible for atomic optional admission.
    Optional,
}

impl RequirementMode {
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::Required => 3,
            Self::DependencyRequired => 2,
            Self::Optional => 1,
        }
    }
}

/// Nonempty, canonically ordered set of B1 roles allowed to see a node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleVisibility {
    roles: Vec<ActorRole>,
}

impl RoleVisibility {
    /// Checks nonemptiness, the configured bound, uniqueness, and canonical ordering.
    ///
    /// # Errors
    ///
    /// Returns a typed collection or bound error.
    pub fn new(roles: Vec<ActorRole>, limits: ContextLimits) -> Result<Self, ContextError> {
        if roles.is_empty() {
            return Err(ContextError::plain(ContextErrorKind::EmptyCollection));
        }
        if roles.len() > limits.max_visibility_roles() {
            return Err(ContextError::with_numbers(
                ContextErrorKind::TooManyVisibilityRoles,
                limits.max_visibility_roles() as u64,
                roles.len() as u64,
            ));
        }
        let mut index = 1;
        while index < roles.len()
            invariant 1 <= index <= roles.len(),
            decreases roles.len() - index,
        {
            if roles[index - 1] == roles[index] {
                return Err(ContextError::plain(ContextErrorKind::DuplicateValue));
            }
            if roles[index - 1] > roles[index] {
                return Err(ContextError::plain(ContextErrorKind::NonCanonicalOrder));
            }
            index += 1;
        }
        Ok(Self { roles })
    }

    /// Borrows the canonical roles.
    #[must_use]
    pub const fn roles(&self) -> &[ActorRole] { self.roles.as_slice() }

    /// Returns whether the canonical B1 role is visible.
    #[must_use]
    pub fn contains(&self, role: ActorRole) -> bool {
        let mut index = 0;
        while index < self.roles.len()
            invariant index <= self.roles.len(),
            decreases self.roles.len() - index,
        {
            if self.roles[index] == role {
                return true;
            }
            index += 1;
        }
        false
    }
}

/// Immutable metadata used by the checked [`ContextNode`] constructor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextNodeMetadata {
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
}

impl ContextNodeMetadata {
    /// Validates all metadata compatibility and canonical dependency invariants.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero estimates/recency, incompatible security labels, or invalid
    /// dependency order, duplication, self-reference, or bounds.
    #[allow(clippy::too_many_arguments, reason = "all security metadata is explicit at the boundary")]
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
        let mut index = 0;
        while index < dependencies.len()
            invariant index <= dependencies.len(),
            decreases dependencies.len() - index,
        {
            if dependencies[index] == id {
                return Err(ContextError::nodes(ContextErrorKind::SelfDependency, id, id));
            }
            if index > 0 {
                if dependencies[index - 1] == dependencies[index] {
                    return Err(ContextError::nodes(
                        ContextErrorKind::DuplicateValue,
                        id,
                        dependencies[index],
                    ));
                }
                if dependencies[index - 1] > dependencies[index] {
                    return Err(ContextError::nodes(
                        ContextErrorKind::NonCanonicalOrder,
                        id,
                        dependencies[index],
                    ));
                }
            }
            index += 1;
        }
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

    /// Returns the stable node ID.
    #[must_use]
    pub const fn id(&self) -> ContextNodeId { self.id }

    #[allow(clippy::missing_const_for_fn, reason = "moving owned vectors is not const-compatible")]
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

/// One immutable, content-bound node in a canonical context DAG.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextNode {
    metadata: ContextNodeMetadata,
    content: ContextContent,
}

impl ContextNode {
    /// Joins checked metadata with digest-verified bounded content.
    #[must_use]
    pub const fn new(metadata: ContextNodeMetadata, content: ContextContent) -> Self {
        Self { metadata, content }
    }

    /// Returns the stable node identifier.
    #[must_use]
    pub const fn id(&self) -> ContextNodeId { self.metadata.id }
    /// Returns the verified content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.content.digest() }
    /// Returns the immutable bounded content.
    #[must_use]
    pub const fn content(&self) -> &ContextContent { &self.content }
    /// Returns the source provenance.
    #[must_use]
    pub const fn provenance(&self) -> Provenance { self.metadata.provenance }
    /// Returns the authority class.
    #[must_use]
    pub const fn authority(&self) -> AuthorityClass { self.metadata.authority }
    /// Returns the trust class.
    #[must_use]
    pub const fn trust(&self) -> TrustClass { self.metadata.trust }
    /// Returns the role-policy context class.
    #[must_use]
    pub const fn context_class(&self) -> ContextClass { self.metadata.context_class }
    /// Returns the semantic content kind.
    #[must_use]
    pub const fn content_kind(&self) -> ContentKind { self.metadata.content_kind }
    /// Returns the caller-supplied positive token estimate.
    #[must_use]
    pub const fn token_estimate(&self) -> u64 { self.metadata.token_estimate }
    /// Returns the caller-supplied positive logical recency sequence.
    #[must_use]
    pub const fn recency_sequence(&self) -> u64 { self.metadata.recency_sequence }
    /// Returns the requirement mode.
    #[must_use]
    pub const fn requirement(&self) -> RequirementMode { self.metadata.requirement }
    /// Returns the explicit optional-ranking priority.
    #[must_use]
    pub const fn priority(&self) -> u16 { self.metadata.priority }
    /// Returns the explicit role visibility set.
    #[must_use]
    pub const fn visibility(&self) -> &RoleVisibility { &self.metadata.visibility }
    /// Returns canonical direct dependency identities.
    #[must_use]
    pub const fn dependencies(&self) -> &[ContextNodeId] {
        self.metadata.dependencies.as_slice()
    }

    pub(crate) fn replace_dependencies(
        &self,
        dependencies: Vec<ContextNodeId>,
        limits: ContextLimits,
    ) -> Result<Self, ContextError> {
        let metadata = ContextNodeMetadata::new(
            self.id(),
            self.provenance(),
            self.authority(),
            self.trust(),
            self.context_class(),
            self.content_kind(),
            self.token_estimate(),
            self.recency_sequence(),
            self.requirement(),
            self.priority(),
            self.visibility().clone(),
            dependencies,
            limits,
        )?;
        Ok(Self::new(metadata, self.content.clone()))
    }
}

const fn kind_matches_authority(kind: ContentKind, authority: AuthorityClass) -> bool {
    match kind {
        ContentKind::SystemPolicy => matches!(authority, AuthorityClass::SystemPolicy),
        ContentKind::ApplicationPolicy => matches!(authority, AuthorityClass::ApplicationPolicy),
        ContentKind::ImmutableSpecification => {
            matches!(authority, AuthorityClass::AcceptanceSpecification)
        }
        ContentKind::ActiveUserInstruction => matches!(authority, AuthorityClass::UserInstruction),
        ContentKind::CapabilityFact => !matches!(authority, AuthorityClass::NonAuthoritative),
        _ => matches!(authority, AuthorityClass::NonAuthoritative),
    }
}

} // verus!
