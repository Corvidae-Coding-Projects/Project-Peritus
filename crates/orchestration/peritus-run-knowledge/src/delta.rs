//! Role-specific changed-fact and current-reference packet planning.

use crate::{
    InvalidationRequest, KnowledgeAuthority, KnowledgeError, KnowledgeErrorKind,
    KnowledgeSection, KnowledgeSectionId, ReuseDecision, RunKnowledgeSnapshot,
    plan_invalidation,
};
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
    const fn new(section_id: KnowledgeSectionId, delivery: DeltaDelivery) -> Self {
        Self { section_id, delivery }
    }

    /// Stable section identity.
    #[must_use]
    pub const fn section_id(self) -> KnowledgeSectionId { self.section_id }

    /// Required packet representation.
    #[must_use]
    pub const fn delivery(self) -> DeltaDelivery { self.delivery }
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
    /// Authoritative facts that must be delivered again.
    #[must_use]
    pub const fn changed_facts(self) -> usize { self.changed_facts }

    /// Authoritative facts safely reused by reference.
    #[must_use]
    pub const fn current_references(self) -> usize { self.current_references }

    /// Navigation-only sections in the packet.
    #[must_use]
    pub const fn navigation_sections(self) -> usize { self.navigation_sections }

    /// Prior sections invalidated before the packet was built.
    #[must_use]
    pub const fn invalidated_prior_sections(self) -> usize {
        self.invalidated_prior_sections
    }
}

/// Complete provider-neutral delta packet for one writer, reviewer, or fixer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaPacket {
    role: HarnessRole,
    candidate: CandidateIdentity,
    entries: Vec<DeltaEntry>,
    accounting: DeltaAccounting,
}

impl DeltaPacket {
    /// Target role.
    #[must_use]
    pub const fn role(&self) -> HarnessRole { self.role }

    /// Exact current candidate.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity { &self.candidate }

    /// Canonically ordered packet entries.
    #[must_use]
    pub const fn entries(&self) -> &[DeltaEntry] { self.entries.as_slice() }

    /// Reuse, refresh, and navigation counts.
    #[must_use]
    pub const fn accounting(&self) -> DeltaAccounting { self.accounting }
}

/// Builds the next role packet from prior and current grounded snapshots.
///
/// # Errors
///
/// Rejects cross-role snapshots or any current section that is stale against the supplied current
/// observations.
pub fn plan_delta_packet(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<DeltaPacket, KnowledgeError> {
    if previous.role() != current.role() {
        return Err(KnowledgeError::plain(KnowledgeErrorKind::RoleMismatch));
    }
    if current.candidate() != request.state().candidate() {
        return Err(KnowledgeError::plain(KnowledgeErrorKind::CurrentSnapshotStale));
    }
    let current_request = InvalidationRequest::same_revision(request.state().clone());
    let current_plan = plan_invalidation(current, &current_request)?;
    let current_entries = current_plan.entries();
    let mut current_index = 0;
    while current_index < current_entries.len()
        invariant current_index <= current_entries.len(),
        decreases current_entries.len() - current_index,
    {
        if current_entries[current_index].decision() != ReuseDecision::Reuse {
            return Err(KnowledgeError::section(
                KnowledgeErrorKind::CurrentSnapshotStale,
                current_entries[current_index].section_id(),
            ));
        }
        current_index += 1;
    }

    let prior_plan = plan_invalidation(previous, request)?;
    let sections = current.sections();
    let mut entries = Vec::with_capacity(sections.len());
    let mut changed_facts = 0usize;
    let mut current_references = 0usize;
    let mut navigation_sections = 0usize;
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            entries.len() == index,
            changed_facts <= index,
            current_references <= index,
            navigation_sections <= index,
        decreases sections.len() - index,
    {
        let section = &sections[index];
        let delivery = if section.authority() == KnowledgeAuthority::NavigationOnly {
            navigation_sections += 1;
            DeltaDelivery::Navigation
        } else if prior_plan.is_reused(section.id()) && prior_material_matches(previous, section) {
            current_references += 1;
            DeltaDelivery::CurrentReference
        } else {
            changed_facts += 1;
            DeltaDelivery::ChangedFact
        };
        entries.push(DeltaEntry::new(section.id(), delivery));
        index += 1;
    }
    Ok(DeltaPacket {
        role: current.role(),
        candidate: *current.candidate(),
        entries,
        accounting: DeltaAccounting {
            changed_facts,
            current_references,
            navigation_sections,
            invalidated_prior_sections: prior_plan.accounting().invalidated(),
        },
    })
}

fn same_material(previous: &KnowledgeSection, current: &KnowledgeSection) -> bool {
    previous.kind() == current.kind()
        && previous.section_digest() == current.section_digest()
        && previous.binding().sources() == current.binding().sources()
        && previous.dependencies() == current.dependencies()
}

fn prior_material_matches(
    previous: &RunKnowledgeSnapshot,
    current: &KnowledgeSection,
) -> bool {
    let sections = previous.sections();
    let mut index = 0;
    while index < sections.len()
        invariant index <= sections.len(),
        decreases sections.len() - index,
    {
        if sections[index].id() == current.id() {
            return same_material(&sections[index], current);
        }
        index += 1;
    }
    false
}

} // verus!
