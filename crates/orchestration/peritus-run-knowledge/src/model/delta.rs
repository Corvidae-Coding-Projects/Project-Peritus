//! Input-defined delta-packet admission, classification, and accounting.

#[cfg(verus_only)]
mod correspondence;
#[cfg(verus_only)]
mod errors;
#[cfg(verus_only)]
mod plan;

#[cfg(verus_only)]
pub use correspondence::{
    plan_all_reuse_matches, plan_invalidated_count_matches, plan_matches_exact_entries,
    plan_reuse_matches,
};
#[cfg(verus_only)]
pub use errors::{
    current_snapshot_error, delta_planning_error,
    normalized_actual_entry_reuse_matches_freshness,
};
#[cfg(verus_only)]
pub use plan::{
    all_actual_entries_reused, all_entries_reused, current_plan_all_reuse_matches_freshness,
    current_section_fresh, current_snapshot_fresh, exact_plan_entries, exact_plan_reuses,
    modeled_dependency_was_invalidated, modeled_plan_reuses, modeled_planned_decision,
    normalized_direct_reuse_matches_freshness, normalized_plan_reuse_matches_freshness,
    normalized_same_revision, plan_entries_after_push, plan_entries_match, plan_entry_match_at,
    section_sources_current,
};

#[cfg(verus_only)]
use crate::{
    DeltaDelivery, DeltaEntry, InvalidationRequest, KnowledgeAuthority, KnowledgeSection,
    KnowledgeSectionId, ReuseDecision, RunKnowledgeSnapshot,
};
use vstd::prelude::*;

verus! {

/// Complete input-defined admission conditions for delta packet planning.
pub open spec fn delta_inputs_valid(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> bool {
    &&& previous.spec_role() == current.spec_role()
    &&& super::candidates_match(
        &current.spec_candidate(),
        &request.spec_state().spec_candidate(),
    )
    &&& current_snapshot_fresh(current, &request.spec_state())
    &&& super::clarification_targets_valid(previous, request)
}

/// Exact input-defined delivery class for one current section.
pub open spec fn delta_delivery(
    previous: &RunKnowledgeSnapshot,
    current: &KnowledgeSection,
    request: &InvalidationRequest,
) -> DeltaDelivery {
    if current.spec_kind().spec_authority() == KnowledgeAuthority::NavigationOnly {
        DeltaDelivery::Navigation
    } else if exact_plan_reuses(previous, request, current.spec_id())
        && super::first_prior_material_matches(previous.spec_sections(), current)
    {
        DeltaDelivery::CurrentReference
    } else {
        DeltaDelivery::ChangedFact
    }
}

/// Packet entries correspond position-for-position to every current section.
pub open spec fn all_delta_entries_exact(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<DeltaEntry>,
) -> bool {
    entries.len() == current.spec_sections().len()
        && forall |index: int| 0 <= index < entries.len() ==>
            #[trigger] entries[index].spec_section_id().spec_matches(
                &current.spec_sections()[index].spec_id())
            && entries[index].spec_delivery()
                == delta_delivery(previous, &current.spec_sections()[index], request)
}

/// A constructed packet prefix exactly classifies the corresponding current sections.
pub open spec fn all_delta_entries_exact_prefix(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<DeltaEntry>,
) -> bool {
    entries.len() <= current.spec_sections().len()
        && forall |index: int| 0 <= index < entries.len() ==>
            #[trigger] entries[index].spec_section_id().spec_matches(
                &current.spec_sections()[index].spec_id())
            && entries[index].spec_delivery()
                == delta_delivery(previous, &current.spec_sections()[index], request)
}

/// Selects one exact classification from a constructed packet prefix.
pub proof fn delta_prefix_entry_at(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<DeltaEntry>,
    index: int,
)
    requires
        all_delta_entries_exact_prefix(previous, current, request, entries),
        0 <= index < entries.len(),
    ensures
        entries[index].spec_section_id().spec_matches(
            &current.spec_sections()[index].spec_id()),
        entries[index].spec_delivery()
            == delta_delivery(previous, &current.spec_sections()[index], request),
{
}

/// Extends an exact packet prefix by the next classified current section.
pub proof fn delta_prefix_after_push(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    prior: Seq<DeltaEntry>,
    entry: DeltaEntry,
)
    requires
        all_delta_entries_exact_prefix(previous, current, request, prior),
        prior.len() < current.spec_sections().len(),
        entry.spec_section_id().spec_matches(
            &current.spec_sections()[prior.len() as int].spec_id()),
        entry.spec_delivery() == delta_delivery(
            previous,
            &current.spec_sections()[prior.len() as int],
            request,
        ),
    ensures all_delta_entries_exact_prefix(
        previous, current, request, prior.push(entry)),
{
    assert forall |index: int| 0 <= index < prior.push(entry).len() implies
        #[trigger] prior.push(entry)[index].spec_section_id().spec_matches(
            &current.spec_sections()[index].spec_id())
        && prior.push(entry)[index].spec_delivery()
            == delta_delivery(previous, &current.spec_sections()[index], request) by {
        if index < prior.len() {
            delta_prefix_entry_at(previous, current, request, prior, index);
            assert(prior.push(entry)[index] == prior[index]);
        } else {
            assert(index == prior.len());
            assert(prior.push(entry)[index] == entry);
        }
    }
}

/// A prefix covering every current section is the complete exact packet sequence.
pub proof fn delta_prefix_complete(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<DeltaEntry>,
)
    requires
        all_delta_entries_exact_prefix(previous, current, request, entries),
        entries.len() == current.spec_sections().len(),
    ensures all_delta_entries_exact(previous, current, request, entries),
{
    assert forall |index: int| 0 <= index < entries.len() implies
        #[trigger] entries[index].spec_section_id().spec_matches(
            &current.spec_sections()[index].spec_id())
        && entries[index].spec_delivery()
            == delta_delivery(previous, &current.spec_sections()[index], request) by {
        delta_prefix_entry_at(previous, current, request, entries, index);
    }
}

/// Number of authoritative sections whose material must be delivered again.
pub open spec fn changed_fact_count(entries: Seq<DeltaEntry>) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        changed_fact_count(entries.drop_last())
            + if entries.last().spec_delivery() == DeltaDelivery::ChangedFact {
                1nat
            } else {
                0nat
            }
    }
}

/// Number of authoritative sections safely represented by a current reference.
pub open spec fn current_reference_count(entries: Seq<DeltaEntry>) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        current_reference_count(entries.drop_last())
            + if entries.last().spec_delivery() == DeltaDelivery::CurrentReference {
                1nat
            } else {
                0nat
            }
    }
}

/// Number of navigation-only sections in the packet.
pub open spec fn navigation_count(entries: Seq<DeltaEntry>) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        navigation_count(entries.drop_last())
            + if entries.last().spec_delivery() == DeltaDelivery::Navigation {
                1nat
            } else {
                0nat
            }
    }
}

/// Input-defined count of sections invalidated by the prior snapshot plan.
pub open spec fn invalidated_prior_count(
    previous: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> nat {
    modeled_invalidated_count(exact_plan_entries(
        previous, request, previous.spec_sections().len()))
}

/// Number of invalidated entries in one deterministic model plan.
pub open spec fn modeled_invalidated_count(
    entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        modeled_invalidated_count(entries.drop_last())
            + if !entries.last().1.spec_is_reuse() { 1nat } else { 0nat }
    }
}

/// Appending one packet entry updates every exact delivery count.
pub proof fn delta_counts_after_push(entries: Seq<DeltaEntry>, entry: DeltaEntry)
    ensures
        changed_fact_count(entries.push(entry)) == changed_fact_count(entries)
            + if entry.spec_delivery() == DeltaDelivery::ChangedFact { 1nat } else { 0nat },
        current_reference_count(entries.push(entry)) == current_reference_count(entries)
            + if entry.spec_delivery() == DeltaDelivery::CurrentReference { 1nat } else { 0nat },
        navigation_count(entries.push(entry)) == navigation_count(entries)
            + if entry.spec_delivery() == DeltaDelivery::Navigation { 1nat } else { 0nat },
{
    assert(entries.push(entry).drop_last() == entries);
    assert(entries.push(entry).last() == entry);
}

} // verus!
