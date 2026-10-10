//! Canonical role-specific run-knowledge snapshots.

mod validation;

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, KnowledgeSection, KnowledgeSectionId,
    KnowledgeSectionKind,
};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;

#[doc = " Complete bounded knowledge retained for one writer, reviewer, or fixer view."]
#[derive(Debug, Eq, PartialEq)]
pub struct RunKnowledgeSnapshot {
    candidate: CandidateIdentity,
    role: HarnessRole,
    repository_inventory: KnowledgeSectionId,
    relevant_file_map: KnowledgeSectionId,
    requirement_ledger: KnowledgeSectionId,
    sections: Vec<KnowledgeSection>,
    limits: KnowledgeLimits,
}
impl RunKnowledgeSnapshot {
    #[doc = " Creates a canonical dependency-ordered role snapshot."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects missing typed base references, bounds violations, duplicate or unordered sections,"]
    #[doc = " cross-lineage/role bindings, future observations, and missing or forward dependencies."]
    #[allow(clippy::too_many_arguments, reason = "base knowledge references remain explicit")]
    pub fn new(
        candidate: CandidateIdentity,
        role: HarnessRole,
        repository_inventory: KnowledgeSectionId,
        relevant_file_map: KnowledgeSectionId,
        requirement_ledger: KnowledgeSectionId,
        sections: Vec<KnowledgeSection>,
        limits: KnowledgeLimits,
    ) -> Result<Self, KnowledgeError> {
        if !crate::binding::supported_role(role) {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::UnsupportedRole));
        }
        if sections.len() < 3 {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::EmptyCollection));
        }
        if sections.len() > limits.max_sections() {
            return Err(KnowledgeError::numbers(
                KnowledgeErrorKind::LimitExceeded,
                limits.max_sections() as u64,
                sections.len() as u64,
            ));
        }
        validation::validate_sections(&candidate, role, sections.as_slice())?;
        if !validation::reference_has_kind(
            sections.as_slice(),
            repository_inventory,
            KnowledgeSectionKind::RepositoryInventory,
        ) || !validation::reference_has_kind(
            sections.as_slice(),
            relevant_file_map,
            KnowledgeSectionKind::RelevantFileMap,
        ) || !validation::reference_has_kind(
            sections.as_slice(),
            requirement_ledger,
            KnowledgeSectionKind::LiteralRequirementLedger,
        ) {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::InvalidRequiredSection));
        }
        Ok(Self {
            candidate,
            role,
            repository_inventory,
            relevant_file_map,
            requirement_ledger,
            sections,
            limits,
        })
    }
    #[doc = " Current candidate against which retained knowledge is evaluated."]
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity {
        &self.candidate
    }
    #[doc = " Exact role-specific view."]
    #[must_use]
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    #[doc = " Typed repository inventory section."]
    #[must_use]
    pub const fn repository_inventory(&self) -> KnowledgeSectionId {
        self.repository_inventory
    }
    #[doc = " Typed relevant-file-map section."]
    #[must_use]
    pub const fn relevant_file_map(&self) -> KnowledgeSectionId {
        self.relevant_file_map
    }
    #[doc = " Reference to the literal public requirement ledger."]
    #[must_use]
    pub const fn requirement_ledger(&self) -> KnowledgeSectionId {
        self.requirement_ledger
    }
    #[doc = " All typed sections in canonical dependency order."]
    #[must_use]
    pub const fn sections(&self) -> &[KnowledgeSection] {
        self.sections.as_slice()
    }
    #[doc = " Bounds under which this snapshot was checked."]
    #[must_use]
    pub const fn limits(&self) -> KnowledgeLimits {
        self.limits
    }
    #[doc = " Finds one section by stable identity."]
    #[must_use]
    pub fn section(&self, id: KnowledgeSectionId) -> Option<&KnowledgeSection> {
        let mut index = 0;
        while index < self.sections.len() {
            if self.sections[index].id().matches(&id) {
                let section = &self.sections[index];
                return Some(section);
            }
            index += 1;
        }
        None
    }
}
impl Clone for RunKnowledgeSnapshot {
    fn clone(&self) -> Self {
        let cloned_sections = KnowledgeSection::clone_sequence(self.sections.as_slice());
        Self {
            candidate: self.candidate,
            role: self.role,
            repository_inventory: self.repository_inventory,
            relevant_file_map: self.relevant_file_map,
            requirement_ledger: self.requirement_ledger,
            sections: cloned_sections,
            limits: self.limits,
        }
    }
}
