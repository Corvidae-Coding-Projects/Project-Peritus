//! One typed, dependency-aware run-knowledge section.

use crate::{
    KnowledgeAuthority, KnowledgeBinding, KnowledgeError, KnowledgeErrorKind, KnowledgeLimits,
    KnowledgeSectionId, KnowledgeSectionKind,
};
use peritus_types::Sha256Digest;

#[doc = " Immutable retained knowledge plus the exact provenance needed to judge reuse."]
#[derive(Debug, Eq, PartialEq)]
pub struct KnowledgeSection {
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
    section_digest: Sha256Digest,
    binding: KnowledgeBinding,
    dependencies: Vec<KnowledgeSectionId>,
}
impl KnowledgeSection {
    pub(crate) fn clone_sequence(sections: &[Self]) -> Vec<Self> {
        let mut result = Vec::with_capacity(sections.len());
        let mut index = 0;
        while index < sections.len() {
            result.push(sections[index].clone());
            index += 1;
        }
        result
    }
    #[doc = " Creates a section with canonical dependency identities."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects oversized, duplicate, unordered, or self-referential dependencies."]
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
        crate::identity::validate_section_ids(dependencies.as_slice(), Some(id))?;
        Ok(Self { id, kind, section_digest, binding, dependencies })
    }
    #[doc = " Stable logical section identity."]
    #[must_use]
    pub const fn id(&self) -> KnowledgeSectionId {
        self.id
    }
    #[doc = " Semantic section category."]
    #[must_use]
    pub const fn kind(&self) -> KnowledgeSectionKind {
        self.kind
    }
    #[doc = " Exact digest of the section content supplied to context rendering."]
    #[must_use]
    pub const fn section_digest(&self) -> Sha256Digest {
        self.section_digest
    }
    #[doc = " Complete production provenance."]
    #[must_use]
    pub const fn binding(&self) -> &KnowledgeBinding {
        &self.binding
    }
    #[doc = " Direct knowledge dependencies in canonical order."]
    #[must_use]
    pub const fn dependencies(&self) -> &[KnowledgeSectionId] {
        self.dependencies.as_slice()
    }
    #[doc = " Fixed evidence authority of this section kind."]
    #[must_use]
    pub const fn authority(&self) -> KnowledgeAuthority {
        self.kind.authority()
    }
    #[doc = " Whether this section may satisfy a typed authoritative evidence requirement."]
    #[must_use]
    pub const fn can_satisfy_authoritative_evidence(&self) -> bool {
        crate::verified::authoritative_evidence_allowed(self.authority())
    }
}
impl Clone for KnowledgeSection {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            kind: self.kind,
            section_digest: self.section_digest,
            binding: self.binding.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}
