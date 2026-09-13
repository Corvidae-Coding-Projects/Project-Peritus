//! One typed, dependency-aware run-knowledge section.

use crate::{
    KnowledgeAuthority, KnowledgeBinding, KnowledgeError, KnowledgeErrorKind, KnowledgeLimits,
    KnowledgeSectionId, KnowledgeSectionKind,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Immutable retained knowledge plus the exact provenance needed to judge reuse.
#[derive(Debug, Eq, PartialEq)]
pub struct KnowledgeSection {
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
    section_digest: Sha256Digest,
    binding: KnowledgeBinding,
    dependencies: Vec<KnowledgeSectionId>,
}

impl KnowledgeSection {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_well_formed() }

    /// Intrinsic dependency shape retained without the caller's allocation limit.
    pub open spec fn spec_well_formed(&self) -> bool {
        crate::model::id_members_valid(self.spec_dependencies(), Some(self.spec_id()))
    }

    /// Exact constructor admission for the supplied section identity and dependencies.
    pub open spec fn inputs_valid(
        id: KnowledgeSectionId, dependencies: Seq<KnowledgeSectionId>, limits: KnowledgeLimits,
    ) -> bool {
        dependencies.len() <= limits.spec_max_dependencies_per_section()
            && crate::model::id_members_valid(dependencies, Some(id))
    }

    /// Exact constructor errors, checking the size bound before identity content.
    pub open spec fn construction_error(
        id: KnowledgeSectionId, dependencies: Seq<KnowledgeSectionId>, limits: KnowledgeLimits,
        error: KnowledgeError,
    ) -> bool {
        if dependencies.len() > limits.spec_max_dependencies_per_section() {
            error.spec_numbers(KnowledgeErrorKind::LimitExceeded,
                limits.spec_max_dependencies_per_section() as u64, dependencies.len() as u64)
        } else { crate::model::id_collection_error(dependencies, Some(id), error) }
    }

    /// Logical view of the stable section identity.
    pub closed spec fn spec_id(&self) -> KnowledgeSectionId { self.id }

    /// Logical view of the semantic section kind.
    pub closed spec fn spec_kind(&self) -> KnowledgeSectionKind { self.kind }

    /// Logical view of the exact rendered-content digest.
    pub closed spec fn spec_section_digest(&self) -> Sha256Digest { self.section_digest }

    /// Logical view of the complete section provenance.
    pub closed spec fn spec_binding(&self) -> KnowledgeBinding { self.binding }

    /// Logical view of the canonical direct dependencies.
    pub closed spec fn spec_dependencies(&self) -> Seq<KnowledgeSectionId> { self.dependencies@ }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_id() == right.spec_id()
            && left.spec_kind() == right.spec_kind()
            && left.spec_section_digest() == right.spec_section_digest()
            && KnowledgeBinding::clone_equivalent(&left.spec_binding(), &right.spec_binding())
            && left.spec_dependencies() == right.spec_dependencies()
    }

    /// Elementwise semantic equivalence used by snapshot cloning.
    pub open spec fn sequence_clone_equivalent(
        left: Seq<KnowledgeSection>,
        right: Seq<KnowledgeSection>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| #![auto]
                0 <= index < left.len() ==> Self::clone_equivalent(&left[index], &right[index])
    }

    pub(crate) fn clone_sequence(sections: &[Self]) -> (result: Vec<Self>)
        ensures Self::sequence_clone_equivalent(sections@, result@),
    {
        let mut result = Vec::with_capacity(sections.len());
        let mut index = 0;
        while index < sections.len()
            invariant
                index <= sections.len(),
                result@.len() == index,
                forall |prior: int| #![auto]
                    0 <= prior < index ==>
                        Self::clone_equivalent(&sections@[prior], &result@[prior]),
            decreases sections.len() - index,
        {
            result.push(sections[index].clone());
            index += 1;
        }
        result
    }

    /// Creates a section with canonical dependency identities.
    ///
    /// # Errors
    ///
    /// Rejects oversized, duplicate, unordered, or self-referential dependencies.
    pub fn new(
        id: KnowledgeSectionId,
        kind: KnowledgeSectionKind,
        section_digest: Sha256Digest,
        binding: KnowledgeBinding,
        dependencies: Vec<KnowledgeSectionId>,
        limits: KnowledgeLimits,
    ) -> (result: Result<Self, KnowledgeError>)
        ensures result.is_ok() == Self::inputs_valid(id, dependencies@, limits),
            match result {
            Ok(value) => value.spec_id() == id
                && value.spec_kind() == kind
                && value.spec_section_digest() == section_digest
                && value.spec_binding() == binding
                && value.spec_dependencies() == dependencies@ && value.spec_well_formed(),
            Err(error) => Self::construction_error(id, dependencies@, limits, error),
        },
    {
        if dependencies.len() > limits.max_dependencies_per_section() {
            return Err(KnowledgeError::numbers(
                KnowledgeErrorKind::LimitExceeded,
                limits.max_dependencies_per_section() as u64,
                dependencies.len() as u64,
            ));
        }
        crate::identity::validate_section_ids(dependencies.as_slice(), Some(id))?;
        Ok(Self { id, kind, section_digest, binding, dependencies })
    }

    /// Stable logical section identity.
    #[must_use]
    pub const fn id(&self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_id(),
    { self.id }

    /// Semantic section category.
    #[must_use]
    pub const fn kind(&self) -> (kind: KnowledgeSectionKind)
        ensures
            kind == self.spec_kind(),
            kind.spec_is_clarification_target()
                == self.spec_kind().spec_is_clarification_target(),
            kind.spec_depends_on_conversation()
                == self.spec_kind().spec_depends_on_conversation(),
            kind.spec_depends_on_candidate()
                == self.spec_kind().spec_depends_on_candidate(),
    { self.kind }

    /// Exact digest of the section content supplied to context rendering.
    #[must_use]
    pub const fn section_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_section_digest(),
    { self.section_digest }

    /// Complete production provenance.
    #[must_use]
    pub const fn binding(&self) -> (binding: &KnowledgeBinding)
        ensures
            binding.spec_candidate() == self.spec_binding().spec_candidate(),
            binding.spec_role() == self.spec_binding().spec_role(),
            binding.spec_creation_sequence() == self.spec_binding().spec_creation_sequence(),
            binding.spec_sources() == self.spec_binding().spec_sources(),
    { &self.binding }

    /// Direct knowledge dependencies in canonical order.
    #[must_use]
    pub const fn dependencies(&self) -> (dependencies: &[KnowledgeSectionId])
        ensures dependencies@ == self.spec_dependencies(),
    { self.dependencies.as_slice() }

    /// Fixed evidence authority of this section kind.
    #[must_use]
    pub const fn authority(&self) -> (authority: KnowledgeAuthority)
        ensures authority == self.spec_kind().spec_authority(),
    { self.kind.authority() }

    /// Whether this section may satisfy a typed authoritative evidence requirement.
    #[must_use]
    pub const fn can_satisfy_authoritative_evidence(&self) -> (allowed: bool)
        ensures allowed == (self.spec_kind().spec_authority() == KnowledgeAuthority::Authoritative),
    {
        crate::verified::authoritative_evidence_allowed(self.authority())
    }
}

impl Clone for KnowledgeSection {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
        Self {
            id: self.id,
            kind: self.kind,
            section_digest: self.section_digest,
            binding: self.binding.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}

} // verus!
