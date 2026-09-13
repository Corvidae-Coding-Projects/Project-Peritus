//! Exact invalidation and delta-planner error correspondence.

use super::*;
use crate::{
    CurrentKnowledgeState, InvalidationRequest, KnowledgeError, KnowledgeErrorKind,
    PlannedKnowledge, RunKnowledgeSnapshot,
};
use vstd::prelude::*;

verus! {

/// Exact first stale current section and complete planner error payload.
pub open spec fn current_snapshot_error(
    snapshot: &RunKnowledgeSnapshot,
    state: &CurrentKnowledgeState,
    error: KnowledgeError,
) -> bool {
    exists |index: int| 0 <= index < snapshot.spec_sections().len()
        && (forall |prior: int| 0 <= prior < index ==>
            current_section_fresh(
                snapshot,
                #[trigger] &snapshot.spec_sections()[prior],
                state,
            ))
        && !current_section_fresh(
            snapshot,
            #[trigger] &snapshot.spec_sections()[index],
            state,
        )
        && error.spec_section(
            KnowledgeErrorKind::CurrentSnapshotStale,
            snapshot.spec_sections()[index].spec_id(),
        )
}

/// Exact top-level delta-planning error precedence and complete optional fields.
pub open spec fn delta_planning_error(
    previous: &RunKnowledgeSnapshot,
    current: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    error: KnowledgeError,
) -> bool {
    if previous.spec_role() != current.spec_role() {
        error.spec_plain(KnowledgeErrorKind::RoleMismatch)
    } else if !crate::model::candidates_match(
        &current.spec_candidate(),
        &request.spec_state().spec_candidate(),
    ) {
        error.spec_plain(KnowledgeErrorKind::CurrentSnapshotStale)
    } else if !current_snapshot_fresh(current, &request.spec_state()) {
        current_snapshot_error(current, &request.spec_state(), error)
    } else {
        crate::model::clarification_targets_error(previous, request, error)
    }
}

proof fn all_actual_reused_after_push(
    entries: Seq<PlannedKnowledge>,
    entry: PlannedKnowledge,
)
    ensures all_actual_entries_reused(entries.push(entry))
        == (all_actual_entries_reused(entries) && entry.spec_decision().spec_is_reuse()),
{
    if all_actual_entries_reused(entries.push(entry)) {
        assert forall |index: int| 0 <= index < entries.len() implies
            #[trigger] entries[index].spec_decision().spec_is_reuse() by {
            assert(entries.push(entry)[index] == entries[index]);
        }
        assert(entries.push(entry).last() == entry);
    }
    if all_actual_entries_reused(entries) && entry.spec_decision().spec_is_reuse() {
        assert forall |index: int| 0 <= index < entries.push(entry).len() implies
            #[trigger] entries.push(entry)[index].spec_decision().spec_is_reuse() by {
            if index < entries.len() {
                assert(entries.push(entry)[index] == entries[index]);
            } else {
                assert(index == entries.len());
                assert(entries.push(entry)[index] == entry);
            }
        }
    }
}

/// At the first executable non-reuse decision, normalized planning identifies the exact stale
/// input section.
pub proof fn normalized_actual_entry_reuse_matches_freshness(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    state: CurrentKnowledgeState,
    entries: Seq<PlannedKnowledge>,
    index: int,
)
    requires
        normalized_same_revision(request, state),
        crate::model::entries_are_exact_prefix(snapshot, request, entries),
        0 <= index < entries.len(),
        forall |prior: int| 0 <= prior < index ==>
            #[trigger] entries[prior].spec_decision().spec_is_reuse(),
    ensures entries[index].spec_decision().spec_is_reuse()
        == current_section_fresh(
            snapshot,
            &snapshot.spec_sections()[index],
            &state,
        ),
{
    let prior = entries.take(index);
    let through = entries.take(index + 1);
    crate::model::restrict_exact_prefix(snapshot, request, entries, index);
    crate::model::restrict_exact_prefix(snapshot, request, entries, index + 1);
    plan_matches_exact_entries(snapshot, request, prior);
    plan_matches_exact_entries(snapshot, request, through);
    let model_prior = exact_plan_entries(snapshot, request, index as nat);
    let model_through = exact_plan_entries(snapshot, request, (index + 1) as nat);
    plan_all_reuse_matches(prior, model_prior);
    plan_all_reuse_matches(through, model_through);
    normalized_plan_reuse_matches_freshness(snapshot, request, state, index as nat);
    normalized_plan_reuse_matches_freshness(snapshot, request, state, (index + 1) as nat);
    assert(all_actual_entries_reused(prior)) by {
        assert forall |prior_index: int| 0 <= prior_index < prior.len() implies
            #[trigger] prior[prior_index].spec_decision().spec_is_reuse() by {
            assert(prior[prior_index] == entries[prior_index]);
        }
    }
    assert(through == prior.push(entries[index]));
    all_actual_reused_after_push(prior, entries[index]);
    assert((forall |prior_index: int| 0 <= prior_index < index ==>
        current_section_fresh(
            snapshot,
            #[trigger] &snapshot.spec_sections()[prior_index],
            &state,
        )));
    if current_section_fresh(
        snapshot,
        &snapshot.spec_sections()[index],
        &state,
    ) {
        assert forall |fresh_index: int| 0 <= fresh_index < index + 1 implies
            current_section_fresh(
                snapshot,
                #[trigger] &snapshot.spec_sections()[fresh_index],
                &state,
            ) by {
            if fresh_index < index {
            } else {
                assert(fresh_index == index);
            }
        }
    }
}

} // verus!
