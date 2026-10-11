//! Delta-packet values, exact field views, and semantic cloning.

use crate::KnowledgeSectionId;
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;

#[doc = " How one section enters a role\'s next provider-neutral context packet."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeltaDelivery {
    #[doc = " Exact authoritative material changed and must be delivered again."]
    ChangedFact,
    #[doc = " Exact authoritative material remains current and may be referenced."]
    CurrentReference,
    #[doc = " Non-authoritative navigation text may be delivered only as navigation."]
    Navigation,
}
#[doc = " One deterministic role-packet entry."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeltaEntry {
    section_id: KnowledgeSectionId,
    delivery: DeltaDelivery,
}
impl DeltaEntry {
    pub(super) const fn new(section_id: KnowledgeSectionId, delivery: DeltaDelivery) -> Self {
        Self { section_id, delivery }
    }
    #[doc = " Stable section identity."]
    #[must_use]
    pub const fn section_id(self) -> KnowledgeSectionId {
        self.section_id
    }
    #[doc = " Required packet representation."]
    #[must_use]
    pub const fn delivery(self) -> DeltaDelivery {
        self.delivery
    }
}
#[doc = " Product-visible packet reuse counters."]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeltaAccounting {
    changed_facts: usize,
    current_references: usize,
    navigation_sections: usize,
    invalidated_prior_sections: usize,
}
impl DeltaAccounting {
    pub(super) const fn new(
        changed_facts: usize,
        current_references: usize,
        navigation_sections: usize,
        invalidated_prior_sections: usize,
    ) -> Self {
        Self { changed_facts, current_references, navigation_sections, invalidated_prior_sections }
    }
    #[doc = " Authoritative facts that must be delivered again."]
    #[must_use]
    pub const fn changed_facts(self) -> usize {
        self.changed_facts
    }
    #[doc = " Authoritative facts safely reused by reference."]
    #[must_use]
    pub const fn current_references(self) -> usize {
        self.current_references
    }
    #[doc = " Navigation-only sections in the packet."]
    #[must_use]
    pub const fn navigation_sections(self) -> usize {
        self.navigation_sections
    }
    #[doc = " Prior sections invalidated before the packet was built."]
    #[must_use]
    pub const fn invalidated_prior_sections(self) -> usize {
        self.invalidated_prior_sections
    }
}
#[doc = " Complete provider-neutral delta packet for one writer, reviewer, or fixer."]
#[derive(Debug, Eq, PartialEq)]
pub struct DeltaPacket {
    role: HarnessRole,
    candidate: CandidateIdentity,
    entries: Vec<DeltaEntry>,
    accounting: DeltaAccounting,
}
impl DeltaPacket {
    pub(super) const fn new(
        role: HarnessRole,
        candidate: CandidateIdentity,
        entries: Vec<DeltaEntry>,
        accounting: DeltaAccounting,
    ) -> Self {
        Self { role, candidate, entries, accounting }
    }
    #[doc = " Target role."]
    #[must_use]
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    #[doc = " Exact current candidate."]
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity {
        &self.candidate
    }
    #[doc = " Canonically ordered packet entries."]
    #[must_use]
    pub const fn entries(&self) -> &[DeltaEntry] {
        self.entries.as_slice()
    }
    #[doc = " Reuse, refresh, and navigation counts."]
    #[must_use]
    pub const fn accounting(&self) -> DeltaAccounting {
        self.accounting
    }
}
impl Clone for DeltaPacket {
    fn clone(&self) -> Self {
        Self {
            role: self.role,
            candidate: self.candidate,
            entries: self.entries.clone(),
            accounting: self.accounting,
        }
    }
}
