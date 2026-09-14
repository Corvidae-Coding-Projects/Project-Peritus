//! Checked context nodes and canonical role/dependency metadata.

mod metadata;
mod visibility;

pub use metadata::ContextNodeMetadata;
pub use visibility::RoleVisibility;

use crate::{
    AuthorityClass, ContentKind, ContextContent, ContextError, ContextLimits, ContextNodeId,
    Provenance, TrustClass,
};
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

/// One immutable, content-bound node in a canonical context DAG.
#[derive(Debug, Eq, PartialEq)]
pub struct ContextNode {
    metadata: ContextNodeMetadata,
    content: ContextContent,
}

impl ContextNode {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool {
        self.spec_metadata().spec_well_formed()
    }

    /// Logical view of the complete checked metadata.
    pub closed spec fn spec_metadata(&self) -> ContextNodeMetadata { self.metadata }

    /// Logical view of exact bounded content.
    pub closed spec fn spec_content(&self) -> ContextContent { self.content }

    /// Logical view of the stable node identity.
    pub closed spec fn spec_id(&self) -> ContextNodeId { self.metadata.spec_id() }
    /// Logical view of the exact content digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.content.spec_digest() }
    /// Logical view of exact content bytes.
    pub closed spec fn spec_content_bytes(&self) -> Seq<u8> { self.content.spec_bytes() }
    /// Logical view of source provenance.
    pub closed spec fn spec_provenance(&self) -> Provenance { self.metadata.spec_provenance() }
    /// Logical view of authority.
    pub closed spec fn spec_authority(&self) -> AuthorityClass { self.metadata.spec_authority() }
    /// Logical view of trust.
    pub closed spec fn spec_trust(&self) -> TrustClass { self.metadata.spec_trust() }
    /// Logical view of the role-policy context class.
    pub closed spec fn spec_context_class(&self) -> ContextClass {
        self.metadata.spec_context_class()
    }
    /// Logical view of the content kind.
    pub closed spec fn spec_content_kind(&self) -> ContentKind { self.metadata.spec_content_kind() }
    /// Logical view of the token estimate.
    pub closed spec fn spec_token_estimate(&self) -> u64 { self.metadata.spec_token_estimate() }
    /// Logical view of the recency sequence.
    pub closed spec fn spec_recency_sequence(&self) -> u64 {
        self.metadata.spec_recency_sequence()
    }
    /// Logical view of the requirement mode.
    pub closed spec fn spec_requirement(&self) -> RequirementMode { self.metadata.spec_requirement() }
    /// Logical view of selection priority.
    pub closed spec fn spec_priority(&self) -> u16 { self.metadata.spec_priority() }
    /// Logical view of exact role visibility.
    pub closed spec fn spec_visibility(&self) -> RoleVisibility { self.metadata.spec_visibility() }
    /// Logical view of canonical direct dependencies.
    pub closed spec fn spec_dependencies(&self) -> Seq<ContextNodeId> {
        self.metadata.spec_dependencies()
    }

    pub(crate) proof fn dependency_match_is_unique(&self, left: nat, right: nat)
        requires
            self.invariant(),
            left < self.spec_dependencies().len(),
            right < self.spec_dependencies().len(),
            self.spec_dependencies()[left as int].spec_matches(
                &self.spec_dependencies()[right as int],
            ),
        ensures left == right,
    {
        reveal(ContextNode::spec_dependencies);
        reveal(ContextNode::spec_metadata);
        self.metadata.dependency_match_is_unique(left, right);
    }

    /// Complete semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        &&& ContextNodeMetadata::clone_equivalent(&left.spec_metadata(), &right.spec_metadata())
        &&& ContextContent::clone_equivalent(&left.spec_content(), &right.spec_content())
        &&& left.spec_id() == right.spec_id()
        &&& left.spec_provenance() == right.spec_provenance()
        &&& left.spec_authority() == right.spec_authority()
        &&& left.spec_trust() == right.spec_trust()
        &&& left.spec_context_class() == right.spec_context_class()
        &&& left.spec_content_kind() == right.spec_content_kind()
        &&& left.spec_token_estimate() == right.spec_token_estimate()
        &&& left.spec_recency_sequence() == right.spec_recency_sequence()
        &&& left.spec_requirement() == right.spec_requirement()
        &&& left.spec_priority() == right.spec_priority()
        &&& left.spec_visibility().spec_roles() == right.spec_visibility().spec_roles()
        &&& left.spec_dependencies() == right.spec_dependencies()
        &&& left.spec_content_bytes() == right.spec_content_bytes()
        &&& left.spec_digest() == right.spec_digest()
    }

    /// Complete semantic relation for changing only direct dependencies.
    pub open spec fn dependencies_replaced(
        before: &Self,
        after: &Self,
        dependencies: Seq<ContextNodeId>,
    ) -> bool {
        &&& before.spec_id() == after.spec_id()
        &&& before.spec_provenance() == after.spec_provenance()
        &&& before.spec_authority() == after.spec_authority()
        &&& before.spec_trust() == after.spec_trust()
        &&& before.spec_context_class() == after.spec_context_class()
        &&& before.spec_content_kind() == after.spec_content_kind()
        &&& before.spec_token_estimate() == after.spec_token_estimate()
        &&& before.spec_recency_sequence() == after.spec_recency_sequence()
        &&& before.spec_requirement() == after.spec_requirement()
        &&& before.spec_priority() == after.spec_priority()
        &&& before.spec_visibility().spec_roles() == after.spec_visibility().spec_roles()
        &&& before.spec_content_bytes() == after.spec_content_bytes()
        &&& before.spec_digest() == after.spec_digest()
        &&& after.spec_dependencies() == dependencies
    }

    /// Elementwise semantic equivalence used by graph cloning.
    pub open spec fn sequence_clone_equivalent(
        left: Seq<Self>,
        right: Seq<Self>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| #![auto]
                0 <= index < left.len() ==> Self::clone_equivalent(&left[index], &right[index])
    }

    pub(crate) fn clone_sequence(nodes: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_clone_equivalent(nodes@, result@),
    {
        let mut result = Vec::with_capacity(nodes.len());
        let mut index = 0;
        while index < nodes.len()
            invariant
                index <= nodes.len(),
                result@.len() == index,
                forall |prior: int| #![auto] 0 <= prior < index ==>
                    Self::clone_equivalent(&nodes@[prior], &result@[prior]),
            decreases nodes.len() - index,
        {
            result.push(nodes[index].clone());
            index += 1;
        }
        result
    }

    /// Joins checked metadata with digest-verified bounded content.
    #[must_use]
    pub const fn new(
        metadata: ContextNodeMetadata,
        content: ContextContent,
    ) -> (result: Self)
        ensures
            result.spec_metadata() == metadata,
            result.spec_content() == content,
    {
        proof {
            use_type_invariant(&metadata);
            reveal(ContextNode::invariant);
            reveal(ContextNode::spec_metadata);
        }
        Self { metadata, content }
    }

    /// Returns the stable node identifier.
    #[must_use]
    pub const fn id(&self) -> (id: ContextNodeId)
        ensures id == self.spec_id(),
    {
        proof { reveal(ContextNode::spec_id); }
        self.metadata.id()
    }
    /// Returns the verified content digest.
    #[must_use]
    pub const fn digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_digest(),
    {
        proof { reveal(ContextNode::spec_digest); }
        self.content.digest()
    }
    /// Returns the immutable bounded content.
    #[must_use]
    pub const fn content(&self) -> (content: &ContextContent)
        ensures
            content.spec_bytes() == self.spec_content_bytes(),
            content.spec_digest() == self.spec_digest(),
    {
        proof {
            reveal(ContextNode::spec_content_bytes);
            reveal(ContextNode::spec_digest);
        }
        &self.content
    }
    /// Returns the source provenance.
    #[must_use]
    pub const fn provenance(&self) -> (provenance: Provenance)
        ensures provenance == self.spec_provenance(),
    {
        proof { reveal(ContextNode::spec_provenance); }
        self.metadata.provenance_value()
    }
    /// Returns the authority class.
    #[must_use]
    pub const fn authority(&self) -> (authority: AuthorityClass)
        ensures authority == self.spec_authority(),
    {
        proof { reveal(ContextNode::spec_authority); }
        self.metadata.authority_value()
    }
    /// Returns the trust class.
    #[must_use]
    pub const fn trust(&self) -> (trust: TrustClass)
        ensures trust == self.spec_trust(),
    {
        proof { reveal(ContextNode::spec_trust); }
        self.metadata.trust_value()
    }
    /// Returns the role-policy context class.
    #[must_use]
    pub const fn context_class(&self) -> (class: ContextClass)
        ensures class == self.spec_context_class(),
    {
        proof { reveal(ContextNode::spec_context_class); }
        self.metadata.context_class_value()
    }
    /// Returns the semantic content kind.
    #[must_use]
    pub const fn content_kind(&self) -> (kind: ContentKind)
        ensures kind == self.spec_content_kind(),
    {
        proof { reveal(ContextNode::spec_content_kind); }
        self.metadata.content_kind_value()
    }
    /// Returns the caller-supplied positive token estimate.
    #[must_use]
    pub const fn token_estimate(&self) -> (estimate: u64)
        ensures estimate == self.spec_token_estimate(),
    {
        proof { reveal(ContextNode::spec_token_estimate); }
        self.metadata.token_estimate_value()
    }
    /// Returns the caller-supplied positive logical recency sequence.
    #[must_use]
    pub const fn recency_sequence(&self) -> (sequence: u64)
        ensures sequence == self.spec_recency_sequence(),
    {
        proof { reveal(ContextNode::spec_recency_sequence); }
        self.metadata.recency_sequence_value()
    }
    /// Returns the requirement mode.
    #[must_use]
    pub const fn requirement(&self) -> (requirement: RequirementMode)
        ensures requirement == self.spec_requirement(),
    {
        proof { reveal(ContextNode::spec_requirement); }
        self.metadata.requirement_value()
    }
    /// Returns the explicit optional-ranking priority.
    #[must_use]
    pub const fn priority(&self) -> (priority: u16)
        ensures priority == self.spec_priority(),
    {
        proof { reveal(ContextNode::spec_priority); }
        self.metadata.priority_value()
    }
    /// Returns the explicit role visibility set.
    #[must_use]
    pub const fn visibility(&self) -> (visibility: &RoleVisibility)
        ensures visibility.spec_roles() == self.spec_visibility().spec_roles(),
    {
        proof { reveal(ContextNode::spec_visibility); }
        self.metadata.visibility_value()
    }
    /// Returns canonical direct dependency identities.
    #[must_use]
    pub const fn dependencies(&self) -> (dependencies: &[ContextNodeId])
        ensures dependencies@ == self.spec_dependencies(),
    {
        proof { reveal(ContextNode::spec_dependencies); }
        self.metadata.dependencies_value()
    }

    pub(crate) fn replace_dependencies(
        &self,
        dependencies: Vec<ContextNodeId>,
        limits: ContextLimits,
    ) -> (result: Result<Self, ContextError>)
        ensures match result {
            Ok(after) => Self::dependencies_replaced(self, &after, dependencies@),
            Err(_) => true,
        },
    {
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
        let content = self.content.clone();
        let result = Self::new(metadata, content);
        proof {
            reveal(ContextNode::dependencies_replaced);
            reveal(ContextNode::spec_id);
            reveal(ContextNode::spec_provenance);
            reveal(ContextNode::spec_authority);
            reveal(ContextNode::spec_trust);
            reveal(ContextNode::spec_context_class);
            reveal(ContextNode::spec_content_kind);
            reveal(ContextNode::spec_token_estimate);
            reveal(ContextNode::spec_recency_sequence);
            reveal(ContextNode::spec_requirement);
            reveal(ContextNode::spec_priority);
            reveal(ContextNode::spec_visibility);
            reveal(ContextNode::spec_content_bytes);
            reveal(ContextNode::spec_digest);
            reveal(ContextNode::spec_dependencies);
            reveal(ContextNode::spec_metadata);
            reveal(ContextNode::spec_content);
        }
        Ok(result)
    }
}

impl Clone for ContextNode {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        let metadata = self.metadata.clone();
        let content = self.content.clone();
        proof {
            use_type_invariant(&metadata);
            reveal(ContextNode::invariant);
            reveal(ContextNode::spec_metadata);
        }
        Self { metadata, content }
    }
}

} // verus!
