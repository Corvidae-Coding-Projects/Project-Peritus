//! Canonical role-specific run-knowledge snapshots.

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, KnowledgeSection, KnowledgeSectionId,
    KnowledgeSectionKind,
};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Complete bounded knowledge retained for one writer, reviewer, or fixer view.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Creates a canonical dependency-ordered role snapshot.
    ///
    /// # Errors
    ///
    /// Rejects missing typed base references, bounds violations, duplicate or unordered sections,
    /// cross-lineage/role bindings, future observations, and missing or forward dependencies.
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

        let mut index = 0;
        while index < sections.len()
            invariant index <= sections.len(),
            decreases sections.len() - index,
        {
            let section = &sections[index];
            if index > 0 {
                if sections[index - 1].id() == section.id() {
                    return Err(KnowledgeError::section(
                        KnowledgeErrorKind::DuplicateValue,
                        section.id(),
                    ));
                }
                if sections[index - 1].id() > section.id() {
                    return Err(KnowledgeError::section(
                        KnowledgeErrorKind::NonCanonicalOrder,
                        section.id(),
                    ));
                }
            }
            if !section.binding().candidate().same_lineage(&candidate) {
                return Err(KnowledgeError::section(
                    KnowledgeErrorKind::CandidateLineageMismatch,
                    section.id(),
                ));
            }
            if section.binding().role() != role {
                return Err(KnowledgeError::section(KnowledgeErrorKind::RoleMismatch, section.id()));
            }
            if section.binding().creation_sequence() > candidate.checkpoint_sequence() {
                return Err(KnowledgeError::section(
                    KnowledgeErrorKind::FutureKnowledge,
                    section.id(),
                ));
            }
            let dependencies = section.dependencies();
            let mut dependency_index = 0;
            while dependency_index < dependencies.len()
                invariant dependency_index <= dependencies.len(),
                decreases dependencies.len() - dependency_index,
            {
                match find_section_index(sections.as_slice(), dependencies[dependency_index]) {
                    Some(found) if found < index => {}
                    _ => {
                        return Err(KnowledgeError::section(
                            KnowledgeErrorKind::InvalidDependency,
                            dependencies[dependency_index],
                        ));
                    }
                }
                dependency_index += 1;
            }
            index += 1;
        }

        if !reference_has_kind(
            sections.as_slice(),
            repository_inventory,
            KnowledgeSectionKind::RepositoryInventory,
        ) || !reference_has_kind(
            sections.as_slice(),
            relevant_file_map,
            KnowledgeSectionKind::RelevantFileMap,
        ) || !reference_has_kind(
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

    /// Current candidate against which retained knowledge is evaluated.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity { &self.candidate }

    /// Exact role-specific view.
    #[must_use]
    pub const fn role(&self) -> HarnessRole { self.role }

    /// Typed repository inventory section.
    #[must_use]
    pub const fn repository_inventory(&self) -> KnowledgeSectionId {
        self.repository_inventory
    }

    /// Typed relevant-file-map section.
    #[must_use]
    pub const fn relevant_file_map(&self) -> KnowledgeSectionId { self.relevant_file_map }

    /// Reference to the literal public requirement ledger.
    #[must_use]
    pub const fn requirement_ledger(&self) -> KnowledgeSectionId { self.requirement_ledger }

    /// All typed sections in canonical dependency order.
    #[must_use]
    pub const fn sections(&self) -> &[KnowledgeSection] { self.sections.as_slice() }

    /// Bounds under which this snapshot was checked.
    #[must_use]
    pub const fn limits(&self) -> KnowledgeLimits { self.limits }

    /// Finds one section by stable identity.
    #[must_use]
    pub fn section(&self, id: KnowledgeSectionId) -> Option<&KnowledgeSection> {
        let mut index = 0;
        while index < self.sections.len()
            invariant index <= self.sections.len(),
            decreases self.sections.len() - index,
        {
            if self.sections[index].id() == id {
                return Some(&self.sections[index]);
            }
            if self.sections[index].id() > id {
                return None;
            }
            index += 1;
        }
        None
    }
}

fn find_section_index(
    sections: &[KnowledgeSection],
    id: KnowledgeSectionId,
) -> (result: Option<usize>)
    ensures match result { Some(index) => index < sections.len(), None => true },
{
    let mut index = 0;
    while index < sections.len()
        invariant index <= sections.len(),
        decreases sections.len() - index,
    {
        if sections[index].id() == id {
            return Some(index);
        }
        if sections[index].id() > id {
            return None;
        }
        index += 1;
    }
    None
}

fn reference_has_kind(
    sections: &[KnowledgeSection],
    id: KnowledgeSectionId,
    kind: KnowledgeSectionKind,
) -> bool {
    let mut index = 0;
    while index < sections.len()
        invariant index <= sections.len(),
        decreases sections.len() - index,
    {
        if sections[index].id() == id {
            return sections[index].kind() == kind;
        }
        if sections[index].id() > id {
            return false;
        }
        index += 1;
    }
    false
}

} // verus!
