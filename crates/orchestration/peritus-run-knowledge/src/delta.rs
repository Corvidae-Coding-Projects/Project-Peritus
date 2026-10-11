//! Role-specific changed-fact and current-reference packet planning.

mod material;
mod packet;

pub use packet::{DeltaAccounting, DeltaDelivery, DeltaEntry, DeltaPacket};

use crate::{
    InvalidationRequest, KnowledgeError, KnowledgeErrorKind, KnowledgeSection,
    RunKnowledgeSnapshot, plan_invalidation,
};

#[doc = " Builds the next role packet from prior and current grounded snapshots."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Rejects cross-role snapshots or any current section that is stale against the supplied current"]
#[doc = " observations."]
pub fn plan_delta_packet(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<DeltaPacket, KnowledgeError> {
    if !crate::binding::roles_match(previous.role(), current.role()) {
        let error = KnowledgeError::plain(KnowledgeErrorKind::RoleMismatch);
        return Err(error);
    }
    let candidate_matches =
        material::candidates_match(current.candidate(), request.state().candidate());
    if !candidate_matches {
        let error = KnowledgeError::plain(KnowledgeErrorKind::CurrentSnapshotStale);
        return Err(error);
    }
    if let Err(error) = validate_current_snapshot(current, request) {
        return Err(error);
    }
    let prior_plan = match plan_invalidation(previous, request) {
        Err(error) => {
            return Err(error);
        }
        Ok(plan) => plan,
    };
    Ok(build_delta_packet(previous, current, request, &prior_plan))
}
fn validate_current_snapshot(
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<(), KnowledgeError> {
    let cloned_state = request.state().clone();
    let current_request = InvalidationRequest::same_revision(cloned_state);
    let current_plan = plan_invalidation(current, &current_request)?;
    let current_entries = current_plan.entries();
    let mut current_index = 0;
    while current_index < current_entries.len() {
        let entry = current_entries[current_index];
        if !entry.decision().is_reuse() {
            let error = KnowledgeError::section(
                KnowledgeErrorKind::CurrentSnapshotStale,
                entry.section_id(),
            );
            return Err(error);
        }
        current_index += 1;
    }
    Ok(())
}
fn build_delta_packet(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    prior_plan: &crate::InvalidationPlan,
) -> DeltaPacket {
    let sections = current.sections();
    let mut entries = Vec::with_capacity(sections.len());
    let mut changed_facts = 0usize;
    let mut current_references = 0usize;
    let mut navigation_sections = 0usize;
    let mut index = 0;
    while index < sections.len() {
        let section = &sections[index];
        let delivery = classify_delivery(previous, request, prior_plan, section);
        match delivery {
            DeltaDelivery::CurrentReference => {
                current_references += 1;
            }
            DeltaDelivery::ChangedFact => {
                changed_facts += 1;
            }
            DeltaDelivery::Navigation => {
                navigation_sections += 1;
            }
        }
        let entry = DeltaEntry::new(section.id(), delivery);
        entries.push(entry);
        index += 1;
    }
    let accounting = DeltaAccounting::new(
        changed_facts,
        current_references,
        navigation_sections,
        prior_plan.accounting().invalidated(),
    );
    let packet = DeltaPacket::new(current.role(), *current.candidate(), entries, accounting);
    packet
}
fn classify_delivery(
    previous: &RunKnowledgeSnapshot,
    _request: &InvalidationRequest,
    prior_plan: &crate::InvalidationPlan,
    section: &KnowledgeSection,
) -> DeltaDelivery {
    if section.authority().is_navigation_only() {
        return DeltaDelivery::Navigation;
    }
    let prior_reused = prior_plan.is_reused(section.id());
    if prior_reused && prior_material_matches(previous, section) {
        DeltaDelivery::CurrentReference
    } else {
        DeltaDelivery::ChangedFact
    }
}
fn prior_material_matches(previous: &RunKnowledgeSnapshot, current: &KnowledgeSection) -> bool {
    let sections = previous.sections();
    let mut index = 0;
    while index < sections.len() {
        if sections[index].id().matches(&current.id()) {
            let same = material::same_material(&sections[index], current);
            return same;
        }
        index += 1;
    }
    false
}
