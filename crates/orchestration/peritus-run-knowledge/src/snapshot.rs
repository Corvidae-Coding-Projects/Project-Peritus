//! Canonical role-specific run-knowledge snapshots.

mod validation;

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, KnowledgeSection, KnowledgeSectionId,
    KnowledgeSectionKind,
};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Complete bounded knowledge retained for one writer, reviewer, or fixer view.
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
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool {
        self.spec_well_formed()
    }

    /// Logical view of the snapshot candidate.
    pub closed spec fn spec_candidate(&self) -> CandidateIdentity { self.candidate }

    /// Logical view of the isolated role.
    pub closed spec fn spec_role(&self) -> HarnessRole { self.role }

    /// Logical view of every dependency-ordered section.
    pub closed spec fn spec_sections(&self) -> Seq<KnowledgeSection> { self.sections@ }

    /// Logical view of the required repository inventory reference.
    pub closed spec fn spec_repository_inventory(&self) -> KnowledgeSectionId {
        self.repository_inventory
    }

    /// Logical view of the required relevant-file-map reference.
    pub closed spec fn spec_relevant_file_map(&self) -> KnowledgeSectionId {
        self.relevant_file_map
    }

    /// Logical view of the required literal-requirement reference.
    pub closed spec fn spec_requirement_ledger(&self) -> KnowledgeSectionId {
        self.requirement_ledger
    }

    /// Logical view of the checked allocation bounds.
    pub closed spec fn spec_limits(&self) -> KnowledgeLimits { self.limits }

    /// Canonical identities, exact base references, and backward dependency membership.
    pub open spec fn spec_topology_valid(&self) -> bool {
        crate::model::snapshot_topology(
            self.spec_sections(),
            self.spec_repository_inventory(),
            self.spec_relevant_file_map(),
            self.spec_requirement_ledger(),
        )
    }

    /// All required references resolve to their exact expected section kinds.
    pub open spec fn required_references_valid(
        sections: Seq<KnowledgeSection>, repository_inventory: KnowledgeSectionId,
        relevant_file_map: KnowledgeSectionId, requirement_ledger: KnowledgeSectionId,
    ) -> bool {
        crate::model::required_section_has_kind(sections, repository_inventory, KnowledgeSectionKind::RepositoryInventory)
            && crate::model::required_section_has_kind(sections, relevant_file_map, KnowledgeSectionKind::RelevantFileMap)
            && crate::model::required_section_has_kind(sections, requirement_ledger, KnowledgeSectionKind::LiteralRequirementLedger)
    }

    /// Exact constructor admission; nested source and dependency bounds were checked by their own constructors.
    pub open spec fn inputs_valid(
        candidate: CandidateIdentity, role: HarnessRole, repository_inventory: KnowledgeSectionId,
        relevant_file_map: KnowledgeSectionId, requirement_ledger: KnowledgeSectionId,
        sections: Seq<KnowledgeSection>, limits: KnowledgeLimits,
    ) -> bool {
        crate::KnowledgeBinding::role_supported(role) && 3 <= sections.len() <= limits.spec_max_sections()
            && crate::model::sections_valid(candidate, role, sections)
            && Self::required_references_valid(sections, repository_inventory, relevant_file_map, requirement_ledger)
    }

    /// All checked snapshot inputs and the resulting unique backward topology.
    pub open spec fn spec_well_formed(&self) -> bool {
        Self::inputs_valid(self.spec_candidate(), self.spec_role(), self.spec_repository_inventory(),
            self.spec_relevant_file_map(), self.spec_requirement_ledger(), self.spec_sections(), self.spec_limits())
            && self.spec_topology_valid()
    }

    /// Exact error precedence, including the first rejected section and complete detail fields.
    pub open spec fn construction_error(
        candidate: CandidateIdentity, role: HarnessRole, repository_inventory: KnowledgeSectionId,
        relevant_file_map: KnowledgeSectionId, requirement_ledger: KnowledgeSectionId,
        sections: Seq<KnowledgeSection>, limits: KnowledgeLimits, error: KnowledgeError,
    ) -> bool {
        if !crate::KnowledgeBinding::role_supported(role) {
            error.spec_plain(KnowledgeErrorKind::UnsupportedRole)
        } else if sections.len() < 3 {
            error.spec_plain(KnowledgeErrorKind::EmptyCollection)
        } else if sections.len() > limits.spec_max_sections() {
            error.spec_numbers(KnowledgeErrorKind::LimitExceeded,
                limits.spec_max_sections() as u64, sections.len() as u64)
        } else if !crate::model::sections_valid(candidate, role, sections) {
            crate::model::sections_validation_error(candidate, role, sections, error)
        } else {
            !Self::required_references_valid(sections, repository_inventory, relevant_file_map, requirement_ledger)
                && error.spec_plain(KnowledgeErrorKind::InvalidRequiredSection)
        }
    }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_candidate() == right.spec_candidate()
            && left.spec_role() == right.spec_role()
            && left.spec_repository_inventory() == right.spec_repository_inventory()
            && left.spec_relevant_file_map() == right.spec_relevant_file_map()
            && left.spec_requirement_ledger() == right.spec_requirement_ledger()
            && KnowledgeSection::sequence_clone_equivalent(
                left.spec_sections(), right.spec_sections())
            && left.spec_limits() == right.spec_limits()
    }

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
    ) -> (result: Result<Self, KnowledgeError>)
        ensures result.is_ok() == Self::inputs_valid(candidate, role, repository_inventory,
                relevant_file_map, requirement_ledger, sections@, limits),
            match result {
            Ok(value) => value.spec_candidate() == candidate
                && value.spec_role() == role
                && value.spec_repository_inventory() == repository_inventory
                && value.spec_relevant_file_map() == relevant_file_map
                && value.spec_requirement_ledger() == requirement_ledger
                && value.spec_sections() == sections@
                && value.spec_limits() == limits
                && value.spec_well_formed(),
            Err(error) => Self::construction_error(candidate, role, repository_inventory,
                relevant_file_map, requirement_ledger, sections@, limits, error),
        },
    {
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

    /// Current candidate against which retained knowledge is evaluated.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: &CandidateIdentity)
        ensures *candidate == self.spec_candidate(),
    { &self.candidate }

    /// Exact role-specific view.
    #[must_use]
    pub const fn role(&self) -> (role: HarnessRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Typed repository inventory section.
    #[must_use]
    pub const fn repository_inventory(&self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_repository_inventory(),
    {
        self.repository_inventory
    }

    /// Typed relevant-file-map section.
    #[must_use]
    pub const fn relevant_file_map(&self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_relevant_file_map(),
    { self.relevant_file_map }

    /// Reference to the literal public requirement ledger.
    #[must_use]
    pub const fn requirement_ledger(&self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_requirement_ledger(),
    { self.requirement_ledger }

    /// All typed sections in canonical dependency order.
    #[must_use]
    pub const fn sections(&self) -> (sections: &[KnowledgeSection])
        ensures sections@ == self.spec_sections(),
    { self.sections.as_slice() }

    /// Bounds under which this snapshot was checked.
    #[must_use]
    pub const fn limits(&self) -> (limits: KnowledgeLimits)
        ensures limits == self.spec_limits(),
    { self.limits }

    /// Finds one section by stable identity.
    #[must_use]
    pub fn section(&self, id: KnowledgeSectionId) -> (result: Option<&KnowledgeSection>)
        ensures crate::model::section_lookup_result(self.spec_sections(), id, result),
    {
        let mut index = 0;
        while index < self.sections.len()
            invariant
                index <= self.sections.len(),
                forall |prior: int| 0 <= prior < index ==>
                    !self.sections@[prior].spec_id().spec_matches(&id),
            decreases self.sections.len() - index,
        {
            if self.sections[index].id().matches(&id) {
                let section = &self.sections[index];
                assert(crate::model::section_lookup_result(
                    self.spec_sections(), id, Some(section))) by {
                    let witness = index as int;
                    assert(self.spec_sections()[witness].spec_id().spec_matches(&id));
                    assert(*section == self.spec_sections()[witness]);
                }
                return Some(section);
            }
            assert(!self.sections@[index as int].spec_id().spec_matches(&id));
            index += 1;
        }
        None
    }
}

impl Clone for RunKnowledgeSnapshot {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof {
            use_type_invariant(&*self);
        }
        let cloned_sections = KnowledgeSection::clone_sequence(self.sections.as_slice());
        proof {
            crate::model::clone_equivalent_preserves_snapshot_topology(
                self.spec_sections(),
                cloned_sections@,
                self.spec_repository_inventory(),
                self.spec_relevant_file_map(),
                self.spec_requirement_ledger(),
            );
            assert(crate::model::section_bindings_valid_through(
                self.spec_candidate(), self.spec_role(), cloned_sections@, cloned_sections@.len())) by {
                assert forall |index: int| 0 <= index < cloned_sections@.len() implies
                    #[trigger] crate::model::binding_valid(self.spec_candidate(), self.spec_role(), cloned_sections@[index]) by {
                    assert(KnowledgeSection::clone_equivalent(&self.spec_sections()[index], &cloned_sections@[index]));
                    assert(crate::model::binding_valid(self.spec_candidate(), self.spec_role(), self.spec_sections()[index]));
                }
            }

        }
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

} // verus!
