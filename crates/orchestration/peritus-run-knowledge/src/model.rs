//! Input-defined logical model for run-knowledge freshness and reuse.

mod admission;
mod closure;
mod delta;
mod material;

#[cfg(verus_only)]
pub use admission::{sources_admitted, sources_canonical, sources_canonical_through,
    sources_first_bad_pair, sources_validation_error, id_members_valid, id_members_valid_through,
    id_first_error, id_collection_error, binding_valid, section_bindings_valid_through,
    sections_valid, sections_valid_through, section_order_error, section_binding_error,
    section_dependency_error_at, section_dependency_error, section_validation_error_at,
    sections_validation_error};

#[cfg(verus_only)]
pub use material::{dependencies_match, first_prior_material_matches, section_material_matches, sources_match};
mod topology;

#[cfg(verus_only)]
pub use closure::{
    exact_plan_is_transitively_closed, transitive_dependency_invalidation_propagates,
};
#[cfg(verus_only)]
pub use delta::{
    all_actual_entries_reused, all_delta_entries_exact, all_delta_entries_exact_prefix,
    all_entries_reused, changed_fact_count,
    current_reference_count, current_section_fresh, current_snapshot_fresh, delta_delivery,
    delta_counts_after_push, delta_inputs_valid, exact_plan_entries,
    exact_plan_reuses, invalidated_prior_count, navigation_count,
    current_plan_all_reuse_matches_freshness, delta_prefix_after_push, delta_prefix_complete,
    delta_prefix_entry_at, normalized_plan_reuse_matches_freshness, normalized_same_revision,
    plan_all_reuse_matches,
    plan_entries_after_push, plan_entries_match, plan_invalidated_count_matches,
    plan_matches_exact_entries, normalized_actual_entry_reuse_matches_freshness,
    plan_reuse_matches,
    current_snapshot_error, delta_planning_error,
};
#[cfg(verus_only)]
pub use topology::{
    canonical_implies_unique, clone_equivalent_preserves_snapshot_topology,
    dependencies_precede, dependencies_precede_through,
    dependency_at, dependency_has_backward_edge, dependency_resolves_before,
    direct_dependency_at, reference_has_kind_result,
    required_section_has_kind, section_ids_unique, section_index_result, section_keys,
    sections_canonical, sections_canonical_through, snapshot_topology,
    transitive_invalidation_closed, transitively_depends_on,
};

#[cfg(verus_only)]
use crate::{
    InvalidationReason, InvalidationRequest, KnowledgeChange, KnowledgeError,
    KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId, PlannedKnowledge, ReuseDecision,
    RunKnowledgeSnapshot, SourceDigest,
};
use vstd::prelude::*;

verus! {

/// Whether two candidate observations have the same complete input-defined identity.
pub open spec fn candidates_match(
    left: &peritus_run_settlement::CandidateIdentity,
    right: &peritus_run_settlement::CandidateIdentity,
) -> bool {
    left.spec_same_candidate(right)
        && left.spec_checkpoint_sequence() == right.spec_checkpoint_sequence()
}

/// Whether an exact source identity and digest occurs in the supplied current catalog.
pub open spec fn source_is_current(catalog: Seq<SourceDigest>, source: SourceDigest) -> bool {
    exists |index: int| 0 <= index < catalog.len() && catalog[index].spec_matches(&source)
}

/// Whether a clarification explicitly names an exact section identity.
pub open spec fn clarification_affects(
    affected_sections: Seq<KnowledgeSectionId>,
    id: KnowledgeSectionId,
) -> bool {
    exists |index: int|
        0 <= index < affected_sections.len() && affected_sections[index].spec_matches(&id)
}

/// Whether an exact plan entry records reuse for the requested section identity.
pub open spec fn plan_reuses(entries: Seq<PlannedKnowledge>, id: KnowledgeSectionId) -> bool {
    exists |index: int| 0 <= index < entries.len()
        && entries[index].spec_section_id().spec_matches(&id)
        && entries[index].spec_decision().spec_is_reuse()
}

/// Exact result of looking up a stable section identity.
pub open spec fn section_lookup_result<'a>(
    sections: Seq<KnowledgeSection>,
    id: KnowledgeSectionId,
    result: Option<&'a KnowledgeSection>,
) -> bool {
    match result {
        Some(section) => exists |index: int|
            0 <= index < sections.len()
                && sections[index].spec_id().spec_matches(&id)
                && *section == sections[index],
        None => forall |index: int|
            0 <= index < sections.len() ==> !sections[index].spec_id().spec_matches(&id),
    }
}

/// Every source bound to one section has an exact current catalog entry.
pub open spec fn all_sources_current(
    sources: Seq<SourceDigest>,
    request: &InvalidationRequest,
) -> bool {
    forall |index: int| 0 <= index < sources.len() ==>
        source_is_current(request.spec_state().spec_sources(), #[trigger] sources[index])
}

/// All direct input facts required before dependency propagation permits reuse.
pub open spec fn direct_reuse_inputs(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
) -> bool {
    let binding = section.spec_binding();
    let current = request.spec_state().spec_candidate();
    &&& binding.spec_candidate().spec_same_lineage(&current)
    &&& binding.spec_role() == snapshot.spec_role()
    &&& binding.spec_creation_sequence() <= current.spec_checkpoint_sequence()
    &&& all_sources_current(binding.spec_sources(), request)
    &&& !(request.spec_change() == KnowledgeChange::UserClarification
        && clarification_affects(request.spec_affected_sections(), section.spec_id()))
    &&& (!section.spec_kind().spec_depends_on_conversation()
        || binding.spec_candidate().spec_conversation_revision()
            == current.spec_conversation_revision()
        || request.spec_change() == KnowledgeChange::UserClarification)
    &&& (!section.spec_kind().spec_depends_on_candidate()
        || binding.spec_candidate().spec_same_candidate(&current))
}

/// Exact direct decision and rejection precedence before dependency propagation.
pub open spec fn direct_decision(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
) -> ReuseDecision {
    let binding = section.spec_binding();
    let current = request.spec_state().spec_candidate();
    if !binding.spec_candidate().spec_same_lineage(&current) {
        ReuseDecision::Invalidate(InvalidationReason::CandidateLineageChanged)
    } else if binding.spec_role() != snapshot.spec_role() {
        ReuseDecision::Invalidate(InvalidationReason::RoleIsolation)
    } else if binding.spec_creation_sequence() > current.spec_checkpoint_sequence() {
        ReuseDecision::Invalidate(InvalidationReason::FutureObservation)
    } else if !all_sources_current(binding.spec_sources(), request) {
        ReuseDecision::Invalidate(InvalidationReason::SourceChanged)
    } else if request.spec_change() == KnowledgeChange::UserClarification
        && clarification_affects(request.spec_affected_sections(), section.spec_id())
    {
        ReuseDecision::Invalidate(InvalidationReason::UserClarification)
    } else if section.spec_kind().spec_depends_on_conversation()
        && binding.spec_candidate().spec_conversation_revision()
            != current.spec_conversation_revision()
        && request.spec_change() != KnowledgeChange::UserClarification
    {
        ReuseDecision::Invalidate(InvalidationReason::ConversationRevisionChanged)
    } else if section.spec_kind().spec_depends_on_candidate()
        && !binding.spec_candidate().spec_same_candidate(&current)
    {
        ReuseDecision::Invalidate(InvalidationReason::CandidateRevisionChanged)
    } else {
        ReuseDecision::Reuse
    }
}

/// Direct reuse is permitted exactly when every supplied direct input remains valid.
pub proof fn direct_decision_reuses_iff_inputs(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
)
    ensures
        direct_decision(snapshot, section, request).spec_is_reuse()
            == direct_reuse_inputs(snapshot, section, request),
{
}

/// Whether an already planned direct dependency was invalidated.
pub open spec fn entry_invalidates_dependency(
    entry: PlannedKnowledge,
    dependency: KnowledgeSectionId,
) -> bool {
    entry.spec_section_id().spec_matches(&dependency)
        && !entry.spec_decision().spec_is_reuse()
}

/// Whether an already planned direct dependency was invalidated.
pub open spec fn dependency_was_invalidated(
    section: &KnowledgeSection,
    entries: Seq<PlannedKnowledge>,
) -> bool {
    exists |dependency: int, entry: int|
        0 <= dependency < section.spec_dependencies().len()
            && 0 <= entry < entries.len()
            && entry_invalidates_dependency(
                entries[entry], section.spec_dependencies()[dependency])
}

/// Exact decision after dependency invalidation is propagated.
pub open spec fn planned_decision(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
    prior_entries: Seq<PlannedKnowledge>,
) -> ReuseDecision {
    let direct = direct_decision(snapshot, section, request);
    if direct == ReuseDecision::Reuse && dependency_was_invalidated(section, prior_entries) {
        ReuseDecision::Invalidate(InvalidationReason::DependencyInvalidated)
    } else {
        direct
    }
}

/// Exact input-defined authority for reusing one section after dependency propagation.
pub open spec fn section_reusable(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
    prior_entries: Seq<PlannedKnowledge>,
) -> bool {
    direct_reuse_inputs(snapshot, section, request)
        && !dependency_was_invalidated(section, prior_entries)
}

/// Final reuse is permitted exactly when direct inputs are valid and no dependency is stale.
pub proof fn planned_decision_reuses_iff_inputs(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
    prior_entries: Seq<PlannedKnowledge>,
)
    ensures
        planned_decision(snapshot, section, request, prior_entries).spec_is_reuse()
            == section_reusable(snapshot, section, request, prior_entries),
{
    direct_decision_reuses_iff_inputs(snapshot, section, request);
}

/// Whether one clarification target names an existing requirement or design section.
pub open spec fn clarification_target_valid(
    sections: Seq<KnowledgeSection>,
    target: KnowledgeSectionId,
) -> bool {
    exists |section: int| 0 <= section < sections.len()
        && sections[section].spec_id().spec_matches(&target)
        && sections[section].spec_kind().spec_is_clarification_target()
}

/// Every clarification target names an existing requirement or design section.
pub open spec fn clarification_targets_valid(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> bool {
    request.spec_change() != KnowledgeChange::UserClarification
        || forall |target: int| 0 <= target < request.spec_affected_sections().len() ==>
            clarification_target_valid(
                snapshot.spec_sections(),
                #[trigger] request.spec_affected_sections()[target],
            )
}

/// Exact first invalid clarification target and complete planner error payload.
pub open spec fn clarification_targets_error(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    error: KnowledgeError,
) -> bool {
    request.spec_change() == KnowledgeChange::UserClarification
        && exists |target: int| 0 <= target < request.spec_affected_sections().len()
            && (forall |prior: int| 0 <= prior < target ==>
                clarification_target_valid(
                    snapshot.spec_sections(),
                    #[trigger] request.spec_affected_sections()[prior],
                ))
            && !clarification_target_valid(
                snapshot.spec_sections(),
                #[trigger] request.spec_affected_sections()[target],
            )
            && error.spec_section(
                KnowledgeErrorKind::InvalidClarificationTarget,
                request.spec_affected_sections()[target],
            )
}

/// Number of reused entries in an exact plan sequence.
pub open spec fn reused_count(entries: Seq<PlannedKnowledge>) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        reused_count(entries.drop_last())
            + if entries.last().spec_decision().spec_is_reuse() {
                1nat
            } else {
                0nat
            }
    }
}

/// Number of invalidated entries in an exact plan sequence.
pub open spec fn invalidated_count(entries: Seq<PlannedKnowledge>) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        invalidated_count(entries.drop_last())
            + if !entries.last().spec_decision().spec_is_reuse() {
                1nat
            } else {
                0nat
            }
    }
}

/// Appending one decision updates both exact accounting counts.
pub proof fn counts_after_push(entries: Seq<PlannedKnowledge>, entry: PlannedKnowledge)
    ensures
        reused_count(entries.push(entry)) == reused_count(entries)
            + if entry.spec_decision().spec_is_reuse() { 1nat } else { 0nat },
        invalidated_count(entries.push(entry)) == invalidated_count(entries)
            + if !entry.spec_decision().spec_is_reuse() { 1nat } else { 0nat },
{
    assert(entries.push(entry).drop_last() == entries);
    assert(entries.push(entry).last() == entry);
}

/// A planned prefix contains the exact decision for each corresponding input section.
pub open spec fn entries_are_exact_prefix(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<PlannedKnowledge>,
) -> bool {
    &&& entries.len() <= snapshot.spec_sections().len()
    &&& forall |index: int| 0 <= index < entries.len() ==>
        entries[index].spec_section_id()
                .spec_matches(&snapshot.spec_sections()[index].spec_id())
            && entries[index].spec_decision() == planned_decision(
                snapshot,
                &snapshot.spec_sections()[index],
                request,
                entries.take(index),
            )
}

/// Restricting an actual exact plan preserves exact input-defined prefix correspondence.
pub proof fn restrict_exact_prefix(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
    entries: Seq<PlannedKnowledge>,
    end: int,
)
    requires
        entries_are_exact_prefix(snapshot, request, entries),
        0 <= end <= entries.len(),
    ensures entries_are_exact_prefix(snapshot, request, entries.take(end)),
{
    assert forall |index: int| 0 <= index < entries.take(end).len() implies
        entries.take(end)[index].spec_section_id()
            .spec_matches(&snapshot.spec_sections()[index].spec_id())
        && entries.take(end)[index].spec_decision() == planned_decision(
            snapshot,
            &snapshot.spec_sections()[index],
            request,
            entries.take(end).take(index),
        ) by {
        assert(entries.take(end)[index] == entries[index]);
        assert(entries.take(end).take(index) == entries.take(index));
    }
}

} // verus!
