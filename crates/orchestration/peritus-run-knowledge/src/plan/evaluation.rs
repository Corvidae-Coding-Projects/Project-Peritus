//! Executable input-defined invalidation decisions and lookup proofs.

use super::{InvalidationReason, PlannedKnowledge, ReuseDecision};
use crate::{
    InvalidationRequest, KnowledgeError, KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId,
    RunKnowledgeSnapshot,
};
use vstd::prelude::*;

verus! {

pub(super) fn validate_clarification_targets(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> (result: Result<(), KnowledgeError>)
    ensures
        result.is_ok() == crate::model::clarification_targets_valid(snapshot, request),
        match result {
            Err(error) => crate::model::clarification_targets_error(snapshot, request, error),
            Ok(()) => true,
        },
{
    if !request.change().is_user_clarification() {
        return Ok(());
    }
    assert(request.spec_change() == crate::KnowledgeChange::UserClarification);
    let targets = request.affected_sections();
    let mut index = 0;
    while index < targets.len()
        invariant
            index <= targets.len(),
            targets@ == request.spec_affected_sections(),
            request.spec_change() == crate::KnowledgeChange::UserClarification,
            forall |prior: int| 0 <= prior < index ==>
                crate::model::clarification_target_valid(
                    snapshot.spec_sections(), targets@[prior]),
        decreases targets.len() - index,
    {
        let target = targets[index];
        if !clarification_target_valid(snapshot, target) {
            assert(!crate::model::clarification_targets_valid(snapshot, request)) by {
                let witness = index as int;
                assert(target == targets@[witness]);
                assert(!crate::model::clarification_target_valid(
                    snapshot.spec_sections(), targets@[witness]));
            }
            let error = KnowledgeError::section(
                KnowledgeErrorKind::InvalidClarificationTarget,
                target,
            );
            assert(crate::model::clarification_targets_error(
                snapshot, request, error)) by {
                let first = index as int;
            }
            return Err(error);
        }
        index += 1;
    }
    Ok(())
}

pub(super) fn direct_decision(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
) -> (decision: ReuseDecision)
    ensures decision == crate::model::direct_decision(snapshot, section, request),
{
    let current = request.state().candidate();
    let binding = section.binding();
    if !binding.candidate().same_lineage(current) {
        return ReuseDecision::Invalidate(InvalidationReason::CandidateLineageChanged);
    }
    if !crate::binding::roles_match(binding.role(), snapshot.role()) {
        return ReuseDecision::Invalidate(InvalidationReason::RoleIsolation);
    }
    if binding.creation_sequence() > current.checkpoint_sequence() {
        return ReuseDecision::Invalidate(InvalidationReason::FutureObservation);
    }
    if !all_sources_current(binding.sources(), request) {
        return ReuseDecision::Invalidate(InvalidationReason::SourceChanged);
    }
    if request.change().is_user_clarification() && request.affects(section.id()) {
        return ReuseDecision::Invalidate(InvalidationReason::UserClarification);
    }
    let same_conversation = binding.candidate().conversation_revision()
        == current.conversation_revision();
    if section.kind().depends_on_conversation()
        && !same_conversation
        && !request.change().is_user_clarification()
    {
        return ReuseDecision::Invalidate(InvalidationReason::ConversationRevisionChanged);
    }
    let same_candidate = binding.candidate().same_candidate(current);
    if section.kind().depends_on_candidate() && !same_candidate {
        return ReuseDecision::Invalidate(InvalidationReason::CandidateRevisionChanged);
    }
    ReuseDecision::Reuse
}

pub(super) fn clarification_target_valid(
    snapshot: &RunKnowledgeSnapshot,
    target: KnowledgeSectionId,
) -> (valid: bool)
    ensures valid == crate::model::clarification_target_valid(snapshot.spec_sections(), target),
{
    let sections = snapshot.sections();
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            sections@ == snapshot.spec_sections(),
            forall |prior: int| 0 <= prior < index ==>
                !(sections@[prior].spec_id().spec_matches(&target)
                    && sections@[prior].spec_kind().spec_is_clarification_target()),
        decreases sections.len() - index,
    {
        let section_id_matches = sections[index].id().matches(&target);
        let section_kind = sections[index].kind();
        if section_id_matches && section_kind.is_clarification_target()
        {
            assert(crate::model::clarification_target_valid(
                snapshot.spec_sections(), target)) by {
                let witness = index as int;
                assert(sections@[witness] == snapshot.spec_sections()[witness]);
                assert(sections@[witness].spec_id().spec_matches(&target));
                assert(section_kind.spec_is_clarification_target()
                    == sections@[witness].spec_kind().spec_is_clarification_target());
            }
            return true;
        }
        assert(!(sections@[index as int].spec_id().spec_matches(&target)
            && sections@[index as int].spec_kind().spec_is_clarification_target())) by {
            assert(section_kind.spec_is_clarification_target()
                == sections@[index as int].spec_kind().spec_is_clarification_target());
        }
        index += 1;
    }
    false
}

pub(super) fn all_sources_current(
    sources: &[crate::SourceDigest],
    request: &InvalidationRequest,
) -> (current: bool)
    ensures current == crate::model::all_sources_current(sources@, request),
{
    let mut index = 0;
    while index < sources.len()
        invariant
            index <= sources.len(),
            forall |prior: int| 0 <= prior < index ==>
                crate::model::source_is_current(
                    request.spec_state().spec_sources(),
                    sources@[prior],
                ),
        decreases sources.len() - index,
    {
        if !request.state().source_is_current(sources[index]) {
            return false;
        }
        index += 1;
    }
    true
}

pub(super) fn dependency_was_invalidated(
    section: &KnowledgeSection,
    entries: &[PlannedKnowledge],
) -> (invalidated: bool)
    ensures invalidated == crate::model::dependency_was_invalidated(section, entries@),
{
    let dependencies = section.dependencies();
    let mut dependency_index = 0;
    while dependency_index < dependencies.len()
        invariant
            dependency_index <= dependencies.len(),
            dependencies@ == section.spec_dependencies(),
            forall |prior_dependency: int| 0 <= prior_dependency < dependency_index ==>
                forall |prior_entry: int| 0 <= prior_entry < entries.len() ==>
                    !crate::model::entry_invalidates_dependency(
                        #[trigger] entries@[prior_entry],
                        #[trigger] dependencies@[prior_dependency],
                    ),
        decreases dependencies.len() - dependency_index,
    {
        let mut entry_index = 0;
        while entry_index < entries.len()
            invariant
                dependency_index < dependencies.len(),
                dependencies@ == section.spec_dependencies(),
                entry_index <= entries.len(),
                forall |prior: int| 0 <= prior < entry_index ==>
                    !crate::model::entry_invalidates_dependency(
                        #[trigger] entries@[prior], dependencies@[dependency_index as int]),
            decreases entries.len() - entry_index,
        {
            let entry = entries[entry_index];
            let entry_matches = entry.section_id.matches(&dependencies[dependency_index]);
            let entry_decision = entry.decision();
            if entry_matches && !entry_decision.is_reuse() {
                assert(crate::model::dependency_was_invalidated(section, entries@)) by {
                    let dependency = dependency_index as int;
                    let entry = entry_index as int;
                    assert(crate::model::entry_invalidates_dependency(
                        entries@[entry], dependencies@[dependency]));
                }
                return true;
            }
            assert(!crate::model::entry_invalidates_dependency(
                entries@[entry_index as int],
                dependencies@[dependency_index as int],
            )) by {
                assert(entry_decision.spec_is_reuse()
                    == entries@[entry_index as int].spec_decision().spec_is_reuse());
            }
            entry_index += 1;
        }
        assert(forall |prior_entry: int| 0 <= prior_entry < entries.len() ==>
            !crate::model::entry_invalidates_dependency(
                entries@[prior_entry], dependencies@[dependency_index as int]));
        dependency_index += 1;
    }
    false
}

} // verus!
