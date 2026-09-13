//! Role-specific changed-fact and current-reference packet planning.

mod material;
mod packet;

pub use packet::{DeltaAccounting, DeltaDelivery, DeltaEntry, DeltaPacket};

use crate::{
    InvalidationRequest, KnowledgeError, KnowledgeErrorKind, KnowledgeSection, RunKnowledgeSnapshot,
    plan_invalidation,
};
#[cfg(verus_only)]
use crate::KnowledgeSectionId;
use vstd::prelude::*;

verus! {

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
) -> (result: Result<DeltaPacket, KnowledgeError>)
    ensures
        result.is_ok() == crate::model::delta_inputs_valid(previous, current, request),
        match result {
            Ok(packet) => packet.spec_refines(previous, current, request),
            Err(error) => crate::model::delta_planning_error(
                previous, current, request, error),
        },
{
    if !crate::binding::roles_match(previous.role(), current.role()) {
        assert(!crate::model::delta_inputs_valid(previous, current, request));
        let error = KnowledgeError::plain(KnowledgeErrorKind::RoleMismatch);
        assert(crate::model::delta_planning_error(previous, current, request, error));
        return Err(error);
    }
    let candidate_matches = material::candidates_match(
        current.candidate(), request.state().candidate());
    if !candidate_matches {
        assert(!crate::model::delta_inputs_valid(previous, current, request));
        let error = KnowledgeError::plain(KnowledgeErrorKind::CurrentSnapshotStale);
        assert(crate::model::delta_planning_error(previous, current, request, error));
        return Err(error);
    }
    if let Err(error) = validate_current_snapshot(current, request) {
        assert(crate::model::delta_planning_error(previous, current, request, error));
        return Err(error);
    }
    let prior_plan = match plan_invalidation(previous, request) {
        Err(error) => {
            assert(crate::model::delta_planning_error(previous, current, request, error));
            return Err(error);
        },
        Ok(plan) => plan,
    };
    assert(crate::model::delta_inputs_valid(previous, current, request));
    Ok(build_delta_packet(previous, current, request, &prior_plan))
}

fn validate_current_snapshot(
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> (result: Result<(), KnowledgeError>)
    ensures
        result.is_ok()
            == crate::model::current_snapshot_fresh(current, &request.spec_state()),
        match result {
            Err(error) => crate::model::current_snapshot_error(
                current, &request.spec_state(), error),
            Ok(()) => true,
        },
{
    let cloned_state = request.state().clone();
    let current_request = InvalidationRequest::same_revision(cloned_state);
    assert(crate::model::normalized_same_revision(
        &current_request, request.spec_state()));
    assert(crate::model::clarification_targets_valid(current, &current_request));
    let current_plan = plan_invalidation(current, &current_request)?;
    proof {
        crate::model::plan_matches_exact_entries(
            current, &current_request, current_plan.spec_entries());
        crate::model::current_plan_all_reuse_matches_freshness(
            current,
            &current_request,
            request.spec_state(),
            current_plan.spec_entries(),
        );
    }
    let current_entries = current_plan.entries();
    let mut current_index = 0;
    while current_index < current_entries.len()
        invariant
            current_index <= current_entries.len(),
            current_entries@ == current_plan.spec_entries(),
            crate::model::normalized_same_revision(
                &current_request, request.spec_state()),
            crate::model::entries_are_exact_prefix(
                current, &current_request, current_plan.spec_entries()),
            crate::model::all_actual_entries_reused(current_plan.spec_entries())
                == crate::model::current_snapshot_fresh(current, &request.spec_state()),
            forall |prior: int| 0 <= prior < current_index ==>
                #[trigger] current_entries@[prior].spec_decision().spec_is_reuse(),
            forall |prior: int| 0 <= prior < current_index ==>
                crate::model::current_section_fresh(
                    current,
                    #[trigger] &current.spec_sections()[prior],
                    &request.spec_state(),
                ),
        decreases current_entries.len() - current_index,
    {
        let entry = current_entries[current_index];
        proof {
            crate::model::normalized_actual_entry_reuse_matches_freshness(
                current,
                &current_request,
                request.spec_state(),
                current_plan.spec_entries(),
                current_index as int,
            );
        }
        if !entry.decision().is_reuse() {
            assert(!crate::model::all_actual_entries_reused(current_plan.spec_entries())) by {
                let witness = current_index as int;
            }
            assert(!crate::model::current_section_fresh(
                current,
                &current.spec_sections()[current_index as int],
                &request.spec_state(),
            ));
            assert(entry.spec_section_id().spec_matches(
                &current.spec_sections()[current_index as int].spec_id()));
            proof {
                KnowledgeSectionId::matches_implies_equal(
                    &entry.spec_section_id(),
                    &current.spec_sections()[current_index as int].spec_id(),
                );
            }
            let error = KnowledgeError::section(
                KnowledgeErrorKind::CurrentSnapshotStale,
                entry.section_id(),
            );
            assert(crate::model::current_snapshot_error(
                current, &request.spec_state(), error)) by {
                let first = current_index as int;
            }
            return Err(error);
        }
        assert(crate::model::current_section_fresh(
            current,
            &current.spec_sections()[current_index as int],
            &request.spec_state(),
        ));
        current_index += 1;
    }
    Ok(())
}

fn build_delta_packet(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    prior_plan: &crate::InvalidationPlan,
) -> (packet: DeltaPacket)
    requires
        crate::model::delta_inputs_valid(previous, current, request),
        prior_plan.spec_refines(previous, request),
    ensures packet.spec_refines(previous, current, request),
{
    proof {
        crate::model::plan_matches_exact_entries(
            previous, request, prior_plan.spec_entries());
        crate::model::plan_invalidated_count_matches(
            prior_plan.spec_entries(),
            crate::model::exact_plan_entries(
                previous, request, previous.spec_sections().len()),
        );
    }
    let sections = current.sections();
    let mut entries = Vec::with_capacity(sections.len());
    let mut changed_facts = 0usize;
    let mut current_references = 0usize;
    let mut navigation_sections = 0usize;
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            sections@ == current.spec_sections(),
            prior_plan.spec_refines(previous, request),
            entries.len() == index,
            crate::model::all_delta_entries_exact_prefix(
                previous, current, request, entries@),
            changed_facts as nat == crate::model::changed_fact_count(entries@),
            current_references as nat == crate::model::current_reference_count(entries@),
            navigation_sections as nat == crate::model::navigation_count(entries@),
            changed_facts + current_references + navigation_sections == index,
        decreases sections.len() - index,
    {
        let section = &sections[index];
        let ghost prior_entries = entries@;
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
        proof {
            crate::model::delta_counts_after_push(prior_entries, entry);
        }
        assert(entries@ == prior_entries.push(entry));
        proof {
            KnowledgeSectionId::matches_reflexive(&section.spec_id());
            crate::model::delta_prefix_after_push(
                previous, current, request, prior_entries, entry);
        }
        index += 1;
    }
    let accounting = DeltaAccounting::new(
        changed_facts,
        current_references,
        navigation_sections,
        prior_plan.accounting().invalidated(),
    );
    let packet = DeltaPacket::new(
        current.role(),
        *current.candidate(),
        entries,
        accounting,
    );
    proof {
        crate::model::delta_prefix_complete(
            previous, current, request, packet.spec_entries());
    }
    assert(packet.spec_refines(previous, current, request));
    packet
}

fn classify_delivery(
    previous: &RunKnowledgeSnapshot,
    _request: &InvalidationRequest,
    prior_plan: &crate::InvalidationPlan,
    section: &KnowledgeSection,
) -> (delivery: DeltaDelivery)
    requires prior_plan.spec_refines(previous, _request),
    ensures delivery == crate::model::delta_delivery(previous, section, _request),
{
    if section.authority().is_navigation_only() {
        return DeltaDelivery::Navigation;
    }
    let prior_reused = prior_plan.is_reused(section.id());
    proof {
        crate::model::plan_matches_exact_entries(
            previous, _request, prior_plan.spec_entries());
        crate::model::plan_reuse_matches(
            prior_plan.spec_entries(),
            crate::model::exact_plan_entries(
                previous, _request, previous.spec_sections().len()),
            section.spec_id(),
        );
    }
    if prior_reused && prior_material_matches(previous, section) {
        DeltaDelivery::CurrentReference
    } else {
        DeltaDelivery::ChangedFact
    }
}

fn prior_material_matches(
    previous: &RunKnowledgeSnapshot,
    current: &KnowledgeSection,
) -> (same: bool)
    ensures same == crate::model::first_prior_material_matches(previous.spec_sections(), current),
{
    let sections = previous.sections();
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            sections@ == previous.spec_sections(),
            forall |prior: int| 0 <= prior < index ==>
                !sections@[prior].spec_id().spec_matches(&current.spec_id()),
        decreases sections.len() - index,
    {
        if sections[index].id().matches(&current.id()) {
            let same = material::same_material(&sections[index], current);
            assert(same == crate::model::first_prior_material_matches(sections@, current)) by {
                if same {
                    let witness = index as int;
                } else {
                    assert forall |other: int| 0 <= other < sections.len()
                        && sections@[other].spec_id().spec_matches(&current.spec_id())
                        && (forall |prior: int| 0 <= prior < other ==>
                            !sections@[prior].spec_id().spec_matches(&current.spec_id())) implies
                        !crate::model::section_material_matches(&sections@[other], current) by {
                        if other < index {
                        } else if other > index {
                            assert(sections@[index as int].spec_id().spec_matches(&current.spec_id()));
                        } else {
                            assert(other == index);
                        }
                    }
                }
            }
            return same;
        }
        index += 1;
    }
    false
}

} // verus!
