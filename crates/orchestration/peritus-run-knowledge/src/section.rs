//! One typed, dependency-aware run-knowledge section.

use crate::{
    KnowledgeAuthority, KnowledgeBinding, KnowledgeError, KnowledgeErrorKind, KnowledgeLimits,
    KnowledgeSectionId, KnowledgeSectionKind,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Immutable retained knowledge plus the exact provenance needed to judge reuse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeSection {
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
    section_digest: Sha256Digest,
    binding: KnowledgeBinding,
    dependencies: Vec<KnowledgeSectionId>,
}

impl KnowledgeSection {
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
    ) -> Result<Self, KnowledgeError> {
        if dependencies.len() > limits.max_dependencies_per_section() {
            return Err(KnowledgeError::numbers(
                KnowledgeErrorKind::LimitExceeded,
                limits.max_dependencies_per_section() as u64,
                dependencies.len() as u64,
            ));
        }
        let mut index = 0;
        while index < dependencies.len()
            invariant index <= dependencies.len(),
            decreases dependencies.len() - index,
        {
            if dependencies[index] == id {
                return Err(KnowledgeError::section(KnowledgeErrorKind::SelfDependency, id));
            }
            if index > 0 {
                if dependencies[index - 1] == dependencies[index] {
                    return Err(KnowledgeError::section(
                        KnowledgeErrorKind::DuplicateValue,
                        dependencies[index],
                    ));
                }
                if dependencies[index - 1] > dependencies[index] {
                    return Err(KnowledgeError::section(
                        KnowledgeErrorKind::NonCanonicalOrder,
                        dependencies[index],
                    ));
                }
            }
            index += 1;
        }
        Ok(Self { id, kind, section_digest, binding, dependencies })
    }

    /// Stable logical section identity.
    #[must_use]
    pub const fn id(&self) -> KnowledgeSectionId { self.id }

    /// Semantic section category.
    #[must_use]
    pub const fn kind(&self) -> KnowledgeSectionKind { self.kind }

    /// Exact digest of the section content supplied to context rendering.
    #[must_use]
    pub const fn section_digest(&self) -> Sha256Digest { self.section_digest }

    /// Complete production provenance.
    #[must_use]
    pub const fn binding(&self) -> &KnowledgeBinding { &self.binding }

    /// Direct knowledge dependencies in canonical order.
    #[must_use]
    pub const fn dependencies(&self) -> &[KnowledgeSectionId] { self.dependencies.as_slice() }

    /// Fixed evidence authority of this section kind.
    #[must_use]
    pub const fn authority(&self) -> KnowledgeAuthority { self.kind.authority() }

    /// Whether this section may satisfy a typed authoritative evidence requirement.
    #[must_use]
    pub const fn can_satisfy_authoritative_evidence(&self) -> bool {
        crate::verified::authoritative_evidence_allowed(self.authority())
    }
}

} // verus!
