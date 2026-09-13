//! Transitive dependency invalidation over exact successful plan entries.

#[cfg(verus_only)]
use crate::{InvalidationRequest, PlannedKnowledge, RunKnowledgeSnapshot};
use vstd::prelude::*;

verus! {

proof fn direct_dependency_invalidation_propagates(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<PlannedKnowledge>,
    dependent: nat,
    dependency: nat,
)
    requires
        entries.len() == snapshot.spec_sections().len(),
        super::entries_are_exact_prefix(snapshot, request, entries),
        super::direct_dependency_at(snapshot.spec_sections(), dependent, dependency),
        !entries[dependency as int].spec_decision().spec_is_reuse(),
    ensures !entries[dependent as int].spec_decision().spec_is_reuse(),
{
    let sections = snapshot.spec_sections();
    let dependency_position = choose |position: int|
        0 <= position < sections[dependent as int].spec_dependencies().len()
            && sections[dependent as int].spec_dependencies()[position].spec_matches(
                &sections[dependency as int].spec_id());
    let dependency_id = sections[dependent as int].spec_dependencies()[dependency_position];
    assert(dependency < dependent);
    assert(dependent < entries.len());
    assert(entries[dependency as int].spec_section_id().spec_matches(
        &sections[dependency as int].spec_id()));
    assert(entries[dependency as int].spec_section_id().spec_matches(&dependency_id)) by {
        assert(entries[dependency as int].spec_section_id().spec_bytes()
            == sections[dependency as int].spec_id().spec_bytes());
        assert(dependency_id.spec_bytes()
            == sections[dependency as int].spec_id().spec_bytes());
    }
    assert(entries.take(dependent as int)[dependency as int]
        == entries[dependency as int]);
    assert(super::entry_invalidates_dependency(
        entries.take(dependent as int)[dependency as int], dependency_id));
    assert(super::dependency_was_invalidated(
        &sections[dependent as int], entries.take(dependent as int))) by {
        let dependency_witness = dependency_position;
        let entry_witness = dependency as int;
        assert(super::entry_invalidates_dependency(
            entries.take(dependent as int)[entry_witness],
            sections[dependent as int].spec_dependencies()[dependency_witness],
        ));
    }
    assert(entries[dependent as int].spec_decision() == super::planned_decision(
        snapshot,
        &sections[dependent as int],
        request,
        entries.take(dependent as int),
    ));
}

/// Invalidation propagates across every finite backward dependency path in an exact plan.
pub proof fn transitive_dependency_invalidation_propagates(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<PlannedKnowledge>,
    dependent: nat,
    ancestor: nat,
)
    requires
        entries.len() == snapshot.spec_sections().len(),
        super::entries_are_exact_prefix(snapshot, request, entries),
        super::transitively_depends_on(snapshot.spec_sections(), dependent, ancestor),
        !entries[ancestor as int].spec_decision().spec_is_reuse(),
    ensures !entries[dependent as int].spec_decision().spec_is_reuse(),
    decreases dependent,
{
    if super::direct_dependency_at(snapshot.spec_sections(), dependent, ancestor) {
        direct_dependency_invalidation_propagates(
            snapshot, request, entries, dependent, ancestor);
    } else {
        let middle = choose |middle: nat| ancestor < middle < dependent
            && super::direct_dependency_at(snapshot.spec_sections(), dependent, middle)
            && super::transitively_depends_on(snapshot.spec_sections(), middle, ancestor);
        transitive_dependency_invalidation_propagates(
            snapshot, request, entries, middle, ancestor);
        direct_dependency_invalidation_propagates(
            snapshot, request, entries, dependent, middle);
    }
}

/// A complete exact planner output is invalidation-closed over transitive dependencies.
pub proof fn exact_plan_is_transitively_closed(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<PlannedKnowledge>,
)
    requires
        entries.len() == snapshot.spec_sections().len(),
        super::entries_are_exact_prefix(snapshot, request, entries),
    ensures super::transitive_invalidation_closed(snapshot.spec_sections(), entries),
{
    assert forall |dependent: nat, ancestor: nat|
        #[trigger] super::transitively_depends_on(
            snapshot.spec_sections(), dependent, ancestor)
        && !entries[ancestor as int].spec_decision().spec_is_reuse() implies
            !entries[dependent as int].spec_decision().spec_is_reuse() by {
        transitive_dependency_invalidation_propagates(
            snapshot, request, entries, dependent, ancestor);
    }
}

} // verus!
