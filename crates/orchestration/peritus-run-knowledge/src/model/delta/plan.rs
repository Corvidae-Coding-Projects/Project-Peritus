//! Exact deterministic invalidation-plan model and normalized freshness.

use super::plan_all_reuse_matches;
use crate::{
    CurrentKnowledgeState, InvalidationRequest, KnowledgeSection, KnowledgeSectionId,
    PlannedKnowledge, ReuseDecision, RunKnowledgeSnapshot,
};
use vstd::prelude::*;

verus! {

/// Exact deterministic plan prefix computed solely from a snapshot and request.
pub open spec fn exact_plan_entries(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    end: nat,
) -> Seq<(KnowledgeSectionId, ReuseDecision)>
    decreases end,
{
    if end == 0 {
        Seq::empty()
    } else {
        let prior = exact_plan_entries(snapshot, request, (end - 1) as nat);
        let index = end - 1;
        prior.push((
            snapshot.spec_sections()[index as int].spec_id(),
            modeled_planned_decision(
                snapshot,
                &snapshot.spec_sections()[index as int],
                request,
                prior,
            ),
        ))
    }
}

/// Pointwise semantic equality of exact plan entries.
pub open spec fn plan_entries_match(
    left: Seq<PlannedKnowledge>,
    right: Seq<(KnowledgeSectionId, ReuseDecision)>,
) -> bool {
    left.len() == right.len()
        && forall |index: int| 0 <= index < left.len() ==>
            #[trigger] left[index].spec_section_id().spec_matches(
                &right[index].0)
            && left[index].spec_decision() == right[index].1
}

/// Selects the exact semantic correspondence at one plan position.
pub proof fn plan_entry_match_at(
    actual: Seq<PlannedKnowledge>,
    model: Seq<(KnowledgeSectionId, ReuseDecision)>,
    index: int,
)
    requires
        plan_entries_match(actual, model),
        0 <= index < actual.len(),
    ensures
        actual[index].spec_section_id().spec_matches(&model[index].0),
        actual[index].spec_decision() == model[index].1,
{
}

/// Extends one exact modeled plan prefix by its next input-defined entry.
pub proof fn plan_entries_after_push(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    prior: Seq<PlannedKnowledge>,
    entry: PlannedKnowledge,
)
    requires
        prior.len() < snapshot.spec_sections().len(),
        plan_entries_match(
            prior,
            exact_plan_entries(snapshot, request, prior.len()),
        ),
        entry.spec_section_id().spec_matches(
            &snapshot.spec_sections()[prior.len() as int].spec_id()),
        entry.spec_decision() == modeled_planned_decision(
            snapshot,
            &snapshot.spec_sections()[prior.len() as int],
            request,
            exact_plan_entries(snapshot, request, prior.len()),
        ),
    ensures plan_entries_match(
        prior.push(entry),
        exact_plan_entries(snapshot, request, prior.len() + 1),
    ),
{
    let model_prior = exact_plan_entries(snapshot, request, prior.len());
    assert forall |index: int| 0 <= index < prior.push(entry).len() implies
        #[trigger] prior.push(entry)[index].spec_section_id().spec_matches(
            &exact_plan_entries(snapshot, request, prior.len() + 1)[index].0)
        && prior.push(entry)[index].spec_decision()
            == exact_plan_entries(snapshot, request, prior.len() + 1)[index].1 by {
        if index < prior.len() {
            plan_entry_match_at(prior, model_prior, index);
            assert(prior.push(entry)[index] == prior[index]);
            assert(exact_plan_entries(
                snapshot, request, prior.len() + 1)[index] == model_prior[index]);
        } else {
            assert(index == prior.len());
            assert(prior.push(entry)[index] == entry);
        }
    }
}

/// Every entry in one exact invalidation plan remains reusable.
pub open spec fn all_entries_reused(
    entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
) -> bool {
    forall |index: int| 0 <= index < entries.len() ==>
        #[trigger] entries[index].1.spec_is_reuse()
}

/// Every entry produced by the executable planner remains reusable.
pub open spec fn all_actual_entries_reused(entries: Seq<PlannedKnowledge>) -> bool {
    forall |index: int| 0 <= index < entries.len() ==>
        #[trigger] entries[index].spec_decision().spec_is_reuse()
}

/// Whether an input-defined dependency was invalidated in an earlier model prefix.
pub open spec fn modeled_dependency_was_invalidated(
    section: &KnowledgeSection,
    entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
) -> bool {
    exists |dependency: int, entry: int|
        0 <= dependency < section.spec_dependencies().len()
            && 0 <= entry < entries.len()
            && entries[entry].0.spec_matches(
                &section.spec_dependencies()[dependency])
            && !entries[entry].1.spec_is_reuse()
}

/// Exact input-defined decision after dependency invalidation propagation.
pub open spec fn modeled_planned_decision(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
    prior_entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
) -> ReuseDecision {
    let direct = crate::model::direct_decision(snapshot, section, request);
    if direct == ReuseDecision::Reuse
        && modeled_dependency_was_invalidated(section, prior_entries)
    {
        ReuseDecision::Invalidate(crate::InvalidationReason::DependencyInvalidated)
    } else {
        direct
    }
}

/// A current state contains every source bound to this section.
pub open spec fn section_sources_current(
    section: &KnowledgeSection,
    state: &CurrentKnowledgeState,
) -> bool {
    forall |index: int| 0 <= index < section.spec_binding().spec_sources().len() ==>
        crate::model::source_is_current(
            state.spec_sources(),
            #[trigger] section.spec_binding().spec_sources()[index],
        )
}

/// Exact freshness inputs checked for each section by a normalized same-revision plan.
pub open spec fn current_section_fresh(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    state: &CurrentKnowledgeState,
) -> bool {
    let binding = section.spec_binding();
    let candidate = state.spec_candidate();
    &&& binding.spec_candidate().spec_same_lineage(&candidate)
    &&& binding.spec_role() == snapshot.spec_role()
    &&& binding.spec_creation_sequence() <= candidate.spec_checkpoint_sequence()
    &&& section_sources_current(section, state)
    &&& (!section.spec_kind().spec_depends_on_conversation()
        || binding.spec_candidate().spec_conversation_revision()
            == candidate.spec_conversation_revision())
    &&& (!section.spec_kind().spec_depends_on_candidate()
        || binding.spec_candidate().spec_same_candidate(&candidate))
}

/// Every current snapshot section is fresh against the supplied current observation.
pub open spec fn current_snapshot_fresh(
    snapshot: &RunKnowledgeSnapshot,
    state: &CurrentKnowledgeState,
) -> bool {
    forall |index: int| 0 <= index < snapshot.spec_sections().len() ==>
        current_section_fresh(
            snapshot,
            #[trigger] &snapshot.spec_sections()[index],
            state,
        )
}

/// A request synthesized for current-state validation preserves only the observation.
pub open spec fn normalized_same_revision(
    request: &InvalidationRequest,
    state: CurrentKnowledgeState,
) -> bool {
    &&& request.spec_state().spec_candidate() == state.spec_candidate()
    &&& request.spec_state().spec_sources() == state.spec_sources()
    &&& request.spec_change() == crate::KnowledgeChange::SameRevision
    &&& request.spec_affected_sections().len() == 0
}

/// Normalized current-state validation checks exactly one section's freshness inputs.
pub proof fn normalized_direct_reuse_matches_freshness(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
    state: CurrentKnowledgeState,
)
    requires normalized_same_revision(request, state),
    ensures crate::model::direct_reuse_inputs(snapshot, section, request)
        == current_section_fresh(snapshot, section, &state),
{
}

proof fn all_reused_after_push(
    entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
    entry: (KnowledgeSectionId, ReuseDecision),
)
    ensures all_entries_reused(entries.push(entry))
        == (all_entries_reused(entries) && entry.1.spec_is_reuse()),
{
    if all_entries_reused(entries.push(entry)) {
        assert forall |index: int| 0 <= index < entries.len() implies
            #[trigger] entries[index].1.spec_is_reuse() by {
            assert(entries.push(entry)[index] == entries[index]);
        }
        assert(entries.push(entry).last() == entry);
    }
    if all_entries_reused(entries) && entry.1.spec_is_reuse() {
        assert forall |index: int| 0 <= index < entries.push(entry).len() implies
            #[trigger] entries.push(entry)[index].1.spec_is_reuse() by {
            if index < entries.len() {
                assert(entries.push(entry)[index] == entries[index]);
            } else {
                assert(index == entries.len());
                assert(entries.push(entry)[index] == entry);
            }
        }
    }
}

proof fn all_reused_excludes_dependency_invalidation(
    section: &KnowledgeSection,
    entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
)
    requires all_entries_reused(entries),
    ensures !modeled_dependency_was_invalidated(section, entries),
{
    if modeled_dependency_was_invalidated(section, entries) {
        let dependency = choose |dependency: int|
            #![trigger section.spec_dependencies()[dependency]]
            exists |entry: int| #![auto]
            0 <= dependency < section.spec_dependencies().len()
                && 0 <= entry < entries.len()
                && entries[entry].0.spec_matches(
                    &section.spec_dependencies()[dependency])
                && !entries[entry].1.spec_is_reuse();
        let entry = choose |entry: int|
            0 <= dependency < section.spec_dependencies().len()
                && 0 <= entry < entries.len()
                && entries[entry].0.spec_matches(
                    &section.spec_dependencies()[dependency])
                && !entries[entry].1.spec_is_reuse();
        assert(entries[entry].1.spec_is_reuse());
    }
}

/// A normalized exact plan is entirely reusable exactly when its input prefix is fresh.
pub proof fn normalized_plan_reuse_matches_freshness(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    state: CurrentKnowledgeState,
    end: nat,
)
    requires
        normalized_same_revision(request, state),
        end <= snapshot.spec_sections().len(),
    ensures all_entries_reused(exact_plan_entries(snapshot, request, end))
        == (forall |index: int| 0 <= index < end ==>
            current_section_fresh(
                snapshot,
                #[trigger] &snapshot.spec_sections()[index],
                &state,
            )),
    decreases end,
{
    if end > 0 {
        let prior = exact_plan_entries(snapshot, request, (end - 1) as nat);
        let index = end - 1;
        let section = &snapshot.spec_sections()[index as int];
        let decision = modeled_planned_decision(snapshot, section, request, prior);
        normalized_plan_reuse_matches_freshness(snapshot, request, state, index as nat);
        normalized_direct_reuse_matches_freshness(snapshot, section, request, state);
        crate::model::direct_decision_reuses_iff_inputs(snapshot, section, request);
        all_reused_after_push(prior, (section.spec_id(), decision));
        if all_entries_reused(prior) {
            all_reused_excludes_dependency_invalidation(section, prior);
        }
        assert(decision.spec_is_reuse()
            == (crate::model::direct_reuse_inputs(snapshot, section, request)
                && !modeled_dependency_was_invalidated(section, prior)));
        if all_entries_reused(exact_plan_entries(snapshot, request, end)) {
            assert(all_entries_reused(prior));
            assert(decision.spec_is_reuse());
            assert(crate::model::direct_reuse_inputs(snapshot, section, request));
            assert forall |entry_index: int| 0 <= entry_index < end implies
                current_section_fresh(
                    snapshot,
                    #[trigger] &snapshot.spec_sections()[entry_index],
                    &state,
                ) by {
                if entry_index < index {
                } else {
                    assert(entry_index == index);
                }
            }
        }
        if forall |entry_index: int| 0 <= entry_index < end ==>
            current_section_fresh(
                snapshot,
                #[trigger] &snapshot.spec_sections()[entry_index],
                &state,
            )
        {
            assert forall |entry_index: int| 0 <= entry_index < index implies
                current_section_fresh(
                    snapshot,
                    #[trigger] &snapshot.spec_sections()[entry_index],
                    &state,
                ) by {
            }
            assert(all_entries_reused(prior));
            assert(current_section_fresh(snapshot, section, &state));
            assert(crate::model::direct_reuse_inputs(snapshot, section, request));
            assert(decision.spec_is_reuse());
            assert(all_entries_reused(exact_plan_entries(snapshot, request, end)));
        }
    }
}

/// An executable normalized plan is fully reusable exactly for a fresh snapshot.
pub proof fn current_plan_all_reuse_matches_freshness(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    state: CurrentKnowledgeState,
    entries: Seq<PlannedKnowledge>,
)
    requires
        normalized_same_revision(request, state),
        plan_entries_match(
            entries,
            exact_plan_entries(snapshot, request, snapshot.spec_sections().len()),
        ),
    ensures all_actual_entries_reused(entries) == current_snapshot_fresh(snapshot, &state),
{
    let model = exact_plan_entries(snapshot, request, snapshot.spec_sections().len());
    plan_all_reuse_matches(entries, model);
    normalized_plan_reuse_matches_freshness(
        snapshot, request, state, snapshot.spec_sections().len());
}

/// Exact prior-plan reuse result for one current section identity.
pub open spec fn exact_plan_reuses(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    id: KnowledgeSectionId,
) -> bool {
    modeled_plan_reuses(
        exact_plan_entries(snapshot, request, snapshot.spec_sections().len()),
        id,
    )
}

/// Whether the deterministic model plan reuses one exact section identity.
pub open spec fn modeled_plan_reuses(
    entries: Seq<(KnowledgeSectionId, ReuseDecision)>,
    id: KnowledgeSectionId,
) -> bool {
    exists |index: int| 0 <= index < entries.len()
        && entries[index].0.spec_matches(&id)
        && entries[index].1.spec_is_reuse()
}

} // verus!
