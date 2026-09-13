//! Exact context-node metadata admission and semantic cloning.

use super::{RequirementMode, RoleVisibility};
use crate::{
    AuthorityClass, ContentKind, ContextError, ContextErrorKind, ContextLimits, ContextNodeId,
    Provenance, TrustClass,
};
use peritus_role::ContextClass;
use vstd::prelude::*;

mod model;
mod validation;

verus! {

/// Immutable metadata used by the checked context-node constructor.
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
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_well_formed() }

    /// Logical view of the stable node identity.
    pub closed spec fn spec_id(&self) -> ContextNodeId { self.id }
    /// Logical view of the source provenance.
    pub closed spec fn spec_provenance(&self) -> Provenance { self.provenance }
    /// Logical view of the authority class.
    pub closed spec fn spec_authority(&self) -> AuthorityClass { self.authority }
    /// Logical view of the trust class.
    pub closed spec fn spec_trust(&self) -> TrustClass { self.trust }
    /// Logical view of the role-policy context class.
    pub closed spec fn spec_context_class(&self) -> ContextClass { self.context_class }
    /// Logical view of the semantic content kind.
    pub closed spec fn spec_content_kind(&self) -> ContentKind { self.content_kind }
    /// Logical view of the positive token estimate.
    pub closed spec fn spec_token_estimate(&self) -> u64 { self.token_estimate }
    /// Logical view of the positive recency sequence.
    pub closed spec fn spec_recency_sequence(&self) -> u64 { self.recency_sequence }
    /// Logical view of the requirement mode.
    pub closed spec fn spec_requirement(&self) -> RequirementMode { self.requirement }
    /// Logical view of the optional-selection priority.
    pub closed spec fn spec_priority(&self) -> u16 { self.priority }
    /// Logical view of the complete visibility set.
    pub closed spec fn spec_visibility(&self) -> RoleVisibility { self.visibility }
    /// Logical view of canonical direct dependencies.
    pub closed spec fn spec_dependencies(&self) -> Seq<ContextNodeId> { self.dependencies@ }

    pub(crate) proof fn dependency_match_is_unique(&self, left: nat, right: nat)
        requires
            self.spec_well_formed(),
            left < self.spec_dependencies().len(),
            right < self.spec_dependencies().len(),
            self.spec_dependencies()[left as int].spec_matches(
                &self.spec_dependencies()[right as int],
            ),
        ensures left == right,
    {
        reveal(ContextNodeMetadata::spec_well_formed);
        model::valid_dependency_match_is_unique(
            self.spec_id(),
            self.spec_dependencies(),
            left,
            right,
        );
    }

    /// Canonical non-self dependency identity shape.
    pub open spec fn dependencies_valid(
        id: ContextNodeId,
        dependencies: Seq<ContextNodeId>,
    ) -> bool {
        model::dependencies_valid(id, dependencies)
    }

    /// Intrinsic shape retained after construction or checked compaction trust preservation.
    pub open spec fn spec_well_formed(&self) -> bool {
        self.spec_token_estimate() > 0
            && self.spec_recency_sequence() > 0
            && self.spec_provenance().spec_permits_authority(self.spec_authority())
            && (self.spec_provenance().spec_permits_trust(self.spec_trust())
                || (self.spec_provenance() == Provenance::DerivedCompaction
                    && self.spec_trust() == TrustClass::Trusted))
            && kind_matches_authority_spec(self.spec_content_kind(), self.spec_authority())
            && Self::dependencies_valid(self.spec_id(), self.spec_dependencies())
    }

    /// Exact public constructor admission for the supplied metadata and dependency bound.
    pub open spec fn inputs_valid(
        id: ContextNodeId,
        provenance: Provenance,
        authority: AuthorityClass,
        trust: TrustClass,
        content_kind: ContentKind,
        token_estimate: u64,
        recency_sequence: u64,
        dependencies: Seq<ContextNodeId>,
        limits: ContextLimits,
    ) -> bool {
        token_estimate > 0
            && recency_sequence > 0
            && provenance.spec_permits_authority(authority)
            && provenance.spec_permits_trust(trust)
            && kind_matches_authority_spec(content_kind, authority)
            && dependencies.len() <= limits.spec_max_dependencies_per_node()
            && Self::dependencies_valid(id, dependencies)
    }

    /// Exact first dependency identity failure after fixed metadata checks pass.
    pub open spec fn dependency_error(
        id: ContextNodeId,
        dependencies: Seq<ContextNodeId>,
        error: ContextError,
    ) -> bool {
        model::dependency_error(id, dependencies, error)
    }

    /// Exact public constructor failure and branch priority.
    pub open spec fn construction_error(
        id: ContextNodeId,
        provenance: Provenance,
        authority: AuthorityClass,
        trust: TrustClass,
        content_kind: ContentKind,
        token_estimate: u64,
        recency_sequence: u64,
        dependencies: Seq<ContextNodeId>,
        limits: ContextLimits,
        error: ContextError,
    ) -> bool {
        if token_estimate == 0 {
            error.spec_is_node(ContextErrorKind::ZeroTokenEstimate, id)
        } else if recency_sequence == 0 {
            error.spec_is_node(ContextErrorKind::ZeroRecency, id)
        } else if !provenance.spec_permits_authority(authority) {
            error.spec_is_node(ContextErrorKind::IncompatibleAuthority, id)
        } else if !provenance.spec_permits_trust(trust) {
            error.spec_is_node(ContextErrorKind::IncompatibleTrust, id)
        } else if !kind_matches_authority_spec(content_kind, authority) {
            error.spec_is_node(ContextErrorKind::IncompatibleContentKind, id)
        } else if dependencies.len() > limits.spec_max_dependencies_per_node() {
            error.spec_is_node_numbers(
                ContextErrorKind::TooManyDependencies,
                id,
                limits.spec_max_dependencies_per_node() as u64,
                dependencies.len() as u64,
            )
        } else {
            Self::dependency_error(id, dependencies, error)
        }
    }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_id() == right.spec_id()
            && left.spec_provenance() == right.spec_provenance()
            && left.spec_authority() == right.spec_authority()
            && left.spec_trust() == right.spec_trust()
            && left.spec_context_class() == right.spec_context_class()
            && left.spec_content_kind() == right.spec_content_kind()
            && left.spec_token_estimate() == right.spec_token_estimate()
            && left.spec_recency_sequence() == right.spec_recency_sequence()
            && left.spec_requirement() == right.spec_requirement()
            && left.spec_priority() == right.spec_priority()
            && RoleVisibility::clone_equivalent(
                &left.spec_visibility(),
                &right.spec_visibility(),
            )
            && left.spec_dependencies() == right.spec_dependencies()
    }

    /// Validates all metadata compatibility and canonical dependency invariants.
    ///
    /// # Errors
    ///
    /// Returns the exact first typed metadata or dependency error.
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
    ) -> (result: Result<Self, ContextError>)
        ensures
            result.is_ok() == Self::inputs_valid(
                id, provenance, authority, trust, content_kind, token_estimate,
                recency_sequence, dependencies@, limits,
            ),
            match result {
                Ok(value) => value.spec_id() == id
                    && value.spec_provenance() == provenance
                    && value.spec_authority() == authority
                    && value.spec_trust() == trust
                    && value.spec_context_class() == context_class
                    && value.spec_content_kind() == content_kind
                    && value.spec_token_estimate() == token_estimate
                    && value.spec_recency_sequence() == recency_sequence
                    && value.spec_requirement() == requirement
                    && value.spec_priority() == priority
                    && value.spec_visibility() == visibility
                    && value.spec_dependencies() == dependencies@
                    && value.spec_well_formed(),
                Err(error) => Self::construction_error(
                    id, provenance, authority, trust, content_kind, token_estimate,
                    recency_sequence, dependencies@, limits, error,
                ),
            },
    {
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

    /// Returns the stable node ID.
    #[must_use]
    pub const fn id(&self) -> (id: ContextNodeId)
        ensures id == self.spec_id(),
    { self.id }

    pub(super) const fn provenance_value(&self) -> (value: Provenance)
        ensures value == self.spec_provenance(),
    { self.provenance }

    pub(super) const fn authority_value(&self) -> (value: AuthorityClass)
        ensures value == self.spec_authority(),
    { self.authority }

    pub(super) const fn trust_value(&self) -> (value: TrustClass)
        ensures value == self.spec_trust(),
    { self.trust }

    pub(super) const fn context_class_value(&self) -> (value: ContextClass)
        ensures value == self.spec_context_class(),
    { self.context_class }

    pub(super) const fn content_kind_value(&self) -> (value: ContentKind)
        ensures value == self.spec_content_kind(),
    { self.content_kind }

    pub(super) const fn token_estimate_value(&self) -> (value: u64)
        ensures value == self.spec_token_estimate(),
    { self.token_estimate }

    pub(super) const fn recency_sequence_value(&self) -> (value: u64)
        ensures value == self.spec_recency_sequence(),
    { self.recency_sequence }

    pub(super) const fn requirement_value(&self) -> (value: RequirementMode)
        ensures value == self.spec_requirement(),
    { self.requirement }

    pub(super) const fn priority_value(&self) -> (value: u16)
        ensures value == self.spec_priority(),
    { self.priority }

    pub(super) const fn visibility_value(&self) -> (value: &RoleVisibility)
        ensures value.spec_roles() == self.spec_visibility().spec_roles(),
    { &self.visibility }

    pub(super) const fn dependencies_value(&self) -> (value: &[ContextNodeId])
        ensures value@ == self.spec_dependencies(),
    { self.dependencies.as_slice() }

    pub(crate) fn preserve_compaction_trust(self) -> (result: Self)
        requires self.spec_provenance() == Provenance::DerivedCompaction,
        ensures
            result.spec_id() == self.spec_id(),
            result.spec_provenance() == self.spec_provenance(),
            result.spec_authority() == self.spec_authority(),
            result.spec_trust() == TrustClass::Trusted,
            result.spec_context_class() == self.spec_context_class(),
            result.spec_content_kind() == self.spec_content_kind(),
            result.spec_token_estimate() == self.spec_token_estimate(),
            result.spec_recency_sequence() == self.spec_recency_sequence(),
            result.spec_requirement() == self.spec_requirement(),
            result.spec_priority() == self.spec_priority(),
            result.spec_visibility() == self.spec_visibility(),
            result.spec_dependencies() == self.spec_dependencies(),
            result.spec_well_formed(),
    {
        proof { use_type_invariant(&self); }
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
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
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

pub open spec fn kind_matches_authority_spec(
    kind: ContentKind,
    authority: AuthorityClass,
) -> bool {
    model::kind_matches_authority(kind, authority)
}

pub(super) const fn kind_matches_authority(
    kind: ContentKind,
    authority: AuthorityClass,
) -> (matches: bool)
    ensures matches == kind_matches_authority_spec(kind, authority),
{
    model::kind_matches_authority_exec(kind, authority)
}

} // verus!
