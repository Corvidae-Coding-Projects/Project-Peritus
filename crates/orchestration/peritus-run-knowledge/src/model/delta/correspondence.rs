//! Correspondence between executable invalidation plans and the exact model.

use super::*;
use crate::{KnowledgeSection, KnowledgeSectionId, PlannedKnowledge, ReuseDecision, RunKnowledgeSnapshot};
use vstd::prelude::*;

verus! {

/// Semantically equal plan sequences make the same exact identity reuse decision.
pub proof fn plan_reuse_matches(
    left: Seq<PlannedKnowledge>,
    right: Seq<(KnowledgeSectionId, ReuseDecision)>,
    id: KnowledgeSectionId,
)
    requires plan_entries_match(left, right),
    ensures crate::model::plan_reuses(left, id) == modeled_plan_reuses(right, id),
{
    if crate::model::plan_reuses(left, id) {
        let index = choose |index: int| 0 <= index < left.len()
            && left[index].spec_section_id().spec_matches(&id)
            && left[index].spec_decision().spec_is_reuse();
        plan_entry_match_at(left, right, index);
        KnowledgeSectionId::matches_symmetric(
            &left[index].spec_section_id(), &right[index].0);
        KnowledgeSectionId::matches_transitive(
            &right[index].0, &left[index].spec_section_id(), &id);
        assert(modeled_plan_reuses(right, id)) by {
            let witness = index;
        }
    }
    if modeled_plan_reuses(right, id) {
        let index = choose |index: int| 0 <= index < right.len()
            && right[index].0.spec_matches(&id)
            && right[index].1.spec_is_reuse();
        plan_entry_match_at(left, right, index);
        KnowledgeSectionId::matches_transitive(
            &left[index].spec_section_id(), &right[index].0, &id);
        assert(crate::model::plan_reuses(left, id)) by {
            let witness = index;
        }
    }
}

/// Semantically equal plan sequences agree on whether every entry is reusable.
pub proof fn plan_all_reuse_matches(
    left: Seq<PlannedKnowledge>,
    right: Seq<(KnowledgeSectionId, ReuseDecision)>,
)
    requires plan_entries_match(left, right),
    ensures all_actual_entries_reused(left) == all_entries_reused(right),
{
    if all_actual_entries_reused(left) {
        assert forall |index: int| 0 <= index < right.len() implies
            #[trigger] right[index].1.spec_is_reuse() by {
            plan_entry_match_at(left, right, index);
        }
    }
    if all_entries_reused(right) {
        assert forall |index: int| 0 <= index < left.len() implies
            #[trigger] left[index].spec_decision().spec_is_reuse() by {
            plan_entry_match_at(left, right, index);
        }
    }
}

/// Semantically equal plan sequences have the same invalidation count.
pub proof fn plan_invalidated_count_matches(
    left: Seq<PlannedKnowledge>,
    right: Seq<(KnowledgeSectionId, ReuseDecision)>,
)
    requires plan_entries_match(left, right),
    ensures crate::model::invalidated_count(left) == modeled_invalidated_count(right),
{
    assert(plan_decisions_match(left, right)) by {
        assert forall |index: int| 0 <= index < left.len() implies
            #[trigger] left[index].spec_decision() == right[index].1 by {
            plan_entry_match_at(left, right, index);
        }
    }
    decision_counts_match(left, right);
}

spec fn plan_decisions_match(
    left: Seq<PlannedKnowledge>,
    right: Seq<(KnowledgeSectionId, ReuseDecision)>,
) -> bool {
    left.len() == right.len()
        && forall |index: int| 0 <= index < left.len() ==>
            #[trigger] left[index].spec_decision() == right[index].1
}

proof fn decision_counts_match(
    left: Seq<PlannedKnowledge>,
    right: Seq<(KnowledgeSectionId, ReuseDecision)>,
)
    requires plan_decisions_match(left, right),
    ensures crate::model::invalidated_count(left) == modeled_invalidated_count(right),
    decreases left.len(),
{
    if left.len() > 0 {
        assert(left.len() == right.len());
        assert(right.len() > 0);
        assert(plan_decisions_match(left.drop_last(), right.drop_last())) by {
            assert(left.drop_last().len() == right.drop_last().len());
            assert forall |index: int| 0 <= index < left.drop_last().len() implies
                #[trigger] left.drop_last()[index].spec_decision()
                    == right.drop_last()[index].1 by {
                assert(left.drop_last()[index] == left[index]);
                assert(right.drop_last()[index] == right[index]);
            }
        }
        decision_counts_match(left.drop_last(), right.drop_last());
        assert(left.last().spec_decision() == right.last().1);
    }
}

proof fn dependency_invalidation_matches(
    section: &KnowledgeSection,
    actual: Seq<PlannedKnowledge>,
    model: Seq<(KnowledgeSectionId, ReuseDecision)>,
)
    requires plan_entries_match(actual, model),
    ensures crate::model::dependency_was_invalidated(section, actual)
        == modeled_dependency_was_invalidated(section, model),
{
    if crate::model::dependency_was_invalidated(section, actual) {
        let dependency = choose |dependency: int|
            #![trigger section.spec_dependencies()[dependency]]
            exists |entry: int| #![auto]
            0 <= dependency < section.spec_dependencies().len()
                && 0 <= entry < actual.len()
                && crate::model::entry_invalidates_dependency(
                    actual[entry], section.spec_dependencies()[dependency]);
        let entry = choose |entry: int|
            0 <= dependency < section.spec_dependencies().len()
                && 0 <= entry < actual.len()
                && crate::model::entry_invalidates_dependency(
                    actual[entry], section.spec_dependencies()[dependency]);
        plan_entry_match_at(actual, model, entry);
        KnowledgeSectionId::matches_symmetric(
            &actual[entry].spec_section_id(), &model[entry].0);
        KnowledgeSectionId::matches_transitive(
            &model[entry].0,
            &actual[entry].spec_section_id(),
            &section.spec_dependencies()[dependency],
        );
        assert(modeled_dependency_was_invalidated(section, model)) by {
            let dependency_witness = dependency;
            let entry_witness = entry;
        }
    }
    if modeled_dependency_was_invalidated(section, model) {
        let dependency = choose |dependency: int|
            #![trigger section.spec_dependencies()[dependency]]
            exists |entry: int| #![auto]
            0 <= dependency < section.spec_dependencies().len()
                && 0 <= entry < model.len()
                && model[entry].0.spec_matches(
                    &section.spec_dependencies()[dependency])
                && !model[entry].1.spec_is_reuse();
        let entry = choose |entry: int|
            0 <= dependency < section.spec_dependencies().len()
                && 0 <= entry < model.len()
                && model[entry].0.spec_matches(
                    &section.spec_dependencies()[dependency])
                && !model[entry].1.spec_is_reuse();
        plan_entry_match_at(actual, model, entry);
        KnowledgeSectionId::matches_transitive(
            &actual[entry].spec_section_id(),
            &model[entry].0,
            &section.spec_dependencies()[dependency],
        );
        assert(crate::model::entry_invalidates_dependency(
            actual[entry], section.spec_dependencies()[dependency]));
        assert(crate::model::dependency_was_invalidated(section, actual)) by {
            assert(exists |dependency_witness: int, entry_witness: int|
                0 <= dependency_witness < section.spec_dependencies().len()
                    && 0 <= entry_witness < actual.len()
                    && crate::model::entry_invalidates_dependency(
                        actual[entry_witness],
                        section.spec_dependencies()[dependency_witness],
                    )) by {
                let dependency_witness = dependency;
                let entry_witness = entry;
            }
        }
    }
}

/// Exact prefixes produced by the actual planner match the deterministic model.
pub proof fn plan_matches_exact_entries(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<PlannedKnowledge>,
)
    requires
        entries.len() <= snapshot.spec_sections().len(),
        crate::model::entries_are_exact_prefix(snapshot, request, entries),
    ensures plan_entries_match(
        entries,
        exact_plan_entries(snapshot, request, entries.len()),
    ),
    decreases entries.len(),
{
    if entries.len() > 0 {
        let index = entries.len() - 1;
        let prior = entries.drop_last();
        assert(entries.take(index as int) == prior);
        assert(crate::model::entries_are_exact_prefix(snapshot, request, prior)) by {
            assert forall |prior_index: int| 0 <= prior_index < prior.len() implies
                prior[prior_index].spec_section_id().spec_matches(
                    &snapshot.spec_sections()[prior_index].spec_id())
                && prior[prior_index].spec_decision() == crate::model::planned_decision(
                    snapshot,
                    &snapshot.spec_sections()[prior_index],
                    request,
                    prior.take(prior_index),
                ) by {
                assert(prior[prior_index] == entries[prior_index]);
                assert(prior.take(prior_index) == entries.take(prior_index));
            }
        }
        plan_matches_exact_entries(snapshot, request, prior);
        let model_prior = exact_plan_entries(snapshot, request, index as nat);
        assert(plan_entries_match(prior, model_prior));
        dependency_invalidation_matches(
            &snapshot.spec_sections()[index as int],
            prior,
            model_prior,
        );
        assert(entries[index as int].spec_decision() == crate::model::planned_decision(
            snapshot,
            &snapshot.spec_sections()[index as int],
            request,
            prior,
        ));
        assert(entries[index as int].spec_decision() == modeled_planned_decision(
            snapshot,
            &snapshot.spec_sections()[index as int],
            request,
            model_prior,
        ));
        assert(entries == prior.push(entries[index as int]));
        plan_entries_after_push(snapshot, request, prior, entries[index as int]);
    }
}

} // verus!
