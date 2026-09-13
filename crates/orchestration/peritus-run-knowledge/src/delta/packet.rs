//! Delta-packet values, exact field views, and semantic cloning.

#[cfg(verus_only)]
use crate::{InvalidationRequest, RunKnowledgeSnapshot};
use crate::KnowledgeSectionId;
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// How one section enters a role's next provider-neutral context packet.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeltaDelivery {
    /// Exact authoritative material changed and must be delivered again.
    ChangedFact,
    /// Exact authoritative material remains current and may be referenced.
    CurrentReference,
    /// Non-authoritative navigation text may be delivered only as navigation.
    Navigation,
}

/// One deterministic role-packet entry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeltaEntry {
    section_id: KnowledgeSectionId,
    delivery: DeltaDelivery,
}

impl DeltaEntry {
    /// Logical view of the exact section identity.
    pub closed spec fn spec_section_id(&self) -> KnowledgeSectionId { self.section_id }

    /// Logical view of the exact delivery classification.
    pub closed spec fn spec_delivery(&self) -> DeltaDelivery { self.delivery }

    pub(super) const fn new(
        section_id: KnowledgeSectionId,
        delivery: DeltaDelivery,
    ) -> (result: Self)
        ensures
            result.spec_section_id() == section_id,
            result.spec_delivery() == delivery,
    {
        Self { section_id, delivery }
    }

    /// Stable section identity.
    #[must_use]
    pub const fn section_id(self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_section_id(),
    { self.section_id }

    /// Required packet representation.
    #[must_use]
    pub const fn delivery(self) -> (delivery: DeltaDelivery)
        ensures delivery == self.spec_delivery(),
    { self.delivery }
}

/// Product-visible packet reuse counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeltaAccounting {
    changed_facts: usize,
    current_references: usize,
    navigation_sections: usize,
    invalidated_prior_sections: usize,
}

impl DeltaAccounting {
    /// Logical count of authoritative facts that changed.
    pub closed spec fn spec_changed_facts(&self) -> nat { self.changed_facts as nat }

    /// Logical count of authoritative current references.
    pub closed spec fn spec_current_references(&self) -> nat { self.current_references as nat }

    /// Logical count of navigation-only sections.
    pub closed spec fn spec_navigation_sections(&self) -> nat { self.navigation_sections as nat }

    /// Logical count of invalidated prior sections.
    pub closed spec fn spec_invalidated_prior_sections(&self) -> nat {
        self.invalidated_prior_sections as nat
    }

    pub(super) const fn new(
        changed_facts: usize,
        current_references: usize,
        navigation_sections: usize,
        invalidated_prior_sections: usize,
    ) -> (result: Self)
        ensures
            result.spec_changed_facts() == changed_facts as nat,
            result.spec_current_references() == current_references as nat,
            result.spec_navigation_sections() == navigation_sections as nat,
            result.spec_invalidated_prior_sections() == invalidated_prior_sections as nat,
    {
        Self {
            changed_facts,
            current_references,
            navigation_sections,
            invalidated_prior_sections,
        }
    }

    /// Authoritative facts that must be delivered again.
    #[must_use]
    pub const fn changed_facts(self) -> (count: usize)
        ensures count as nat == self.spec_changed_facts(),
    { self.changed_facts }

    /// Authoritative facts safely reused by reference.
    #[must_use]
    pub const fn current_references(self) -> (count: usize)
        ensures count as nat == self.spec_current_references(),
    { self.current_references }

    /// Navigation-only sections in the packet.
    #[must_use]
    pub const fn navigation_sections(self) -> (count: usize)
        ensures count as nat == self.spec_navigation_sections(),
    { self.navigation_sections }

    /// Prior sections invalidated before the packet was built.
    #[must_use]
    pub const fn invalidated_prior_sections(self) -> (count: usize)
        ensures count as nat == self.spec_invalidated_prior_sections(),
    {
        self.invalidated_prior_sections
    }
}

/// Complete provider-neutral delta packet for one writer, reviewer, or fixer.
#[derive(Debug, Eq, PartialEq)]
pub struct DeltaPacket {
    role: HarnessRole,
    candidate: CandidateIdentity,
    entries: Vec<DeltaEntry>,
    accounting: DeltaAccounting,
}

impl DeltaPacket {
    /// Logical target role.
    pub closed spec fn spec_role(&self) -> HarnessRole { self.role }

    /// Logical current candidate observation.
    pub closed spec fn spec_candidate(&self) -> CandidateIdentity { self.candidate }

    /// Logical ordered packet entries.
    pub closed spec fn spec_entries(&self) -> Seq<DeltaEntry> { self.entries@ }

    /// Logical packet accounting.
    pub closed spec fn spec_accounting(&self) -> DeltaAccounting { self.accounting }

    pub(super) const fn new(
        role: HarnessRole,
        candidate: CandidateIdentity,
        entries: Vec<DeltaEntry>,
        accounting: DeltaAccounting,
    ) -> (result: Self)
        ensures
            result.spec_role() == role,
            result.spec_candidate() == candidate,
            result.spec_entries() == entries@,
            result.spec_accounting() == accounting,
    {
        Self { role, candidate, entries, accounting }
    }

    /// Complete input-defined successful packet result.
    pub open spec fn spec_refines(
        &self,
        previous: &RunKnowledgeSnapshot,
        current: &RunKnowledgeSnapshot,
        request: &InvalidationRequest,
    ) -> bool {
        &&& crate::model::delta_inputs_valid(previous, current, request)
        &&& self.spec_role() == current.spec_role()
        &&& crate::model::candidates_match(
            &self.spec_candidate(), &current.spec_candidate())
        &&& crate::model::all_delta_entries_exact(
            previous, current, request, self.spec_entries())
        &&& self.spec_accounting().spec_changed_facts()
            == crate::model::changed_fact_count(self.spec_entries())
        &&& self.spec_accounting().spec_current_references()
            == crate::model::current_reference_count(self.spec_entries())
        &&& self.spec_accounting().spec_navigation_sections()
            == crate::model::navigation_count(self.spec_entries())
        &&& self.spec_accounting().spec_changed_facts()
            + self.spec_accounting().spec_current_references()
            + self.spec_accounting().spec_navigation_sections()
            == self.spec_entries().len()
        &&& self.spec_accounting().spec_invalidated_prior_sections()
            == crate::model::invalidated_prior_count(previous, request)
    }

    /// Complete semantic content preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_role() == right.spec_role()
            && crate::model::candidates_match(
                &left.spec_candidate(), &right.spec_candidate())
            && left.spec_entries() == right.spec_entries()
            && left.spec_accounting() == right.spec_accounting()
    }

    /// Target role.
    #[must_use]
    pub const fn role(&self) -> (role: HarnessRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Exact current candidate.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: &CandidateIdentity)
        ensures *candidate == self.spec_candidate(),
    { &self.candidate }

    /// Canonically ordered packet entries.
    #[must_use]
    pub const fn entries(&self) -> (entries: &[DeltaEntry])
        ensures entries@ == self.spec_entries(),
    { self.entries.as_slice() }

    /// Reuse, refresh, and navigation counts.
    #[must_use]
    pub const fn accounting(&self) -> (accounting: DeltaAccounting)
        ensures accounting == self.spec_accounting(),
    { self.accounting }
}

impl Clone for DeltaPacket {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            role: self.role,
            candidate: self.candidate,
            entries: self.entries.clone(),
            accounting: self.accounting,
        }
    }
}

} // verus!
