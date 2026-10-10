//! Executable input-defined invalidation decisions and lookup proofs.

use super::{InvalidationReason, PlannedKnowledge, ReuseDecision};
use crate::{
    InvalidationRequest, KnowledgeError, KnowledgeErrorKind, KnowledgeSection, KnowledgeSectionId,
    RunKnowledgeSnapshot,
};

pub(super) fn validate_clarification_targets(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<(), KnowledgeError> {
    if !request.change().is_user_clarification() {
        return Ok(());
    }
    let targets = request.affected_sections();
    let mut index = 0;
    while index < targets.len() {
        let target = targets[index];
        if !clarification_target_valid(snapshot, target) {
            let error =
                KnowledgeError::section(KnowledgeErrorKind::InvalidClarificationTarget, target);
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
) -> ReuseDecision {
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
    let same_conversation =
        binding.candidate().requirements_revision() == current.requirements_revision();
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
) -> bool {
    let sections = snapshot.sections();
    let mut index = 0;
    while index < sections.len() {
        let section_id_matches = sections[index].id().matches(&target);
        let section_kind = sections[index].kind();
        if section_id_matches && section_kind.is_clarification_target() {
            return true;
        }
        index += 1;
    }
    false
}
pub(super) fn all_sources_current(
    sources: &[crate::SourceDigest],
    request: &InvalidationRequest,
) -> bool {
    let mut index = 0;
    while index < sources.len() {
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
) -> bool {
    let dependencies = section.dependencies();
    let mut dependency_index = 0;
    while dependency_index < dependencies.len() {
        let mut entry_index = 0;
        while entry_index < entries.len() {
            let entry = entries[entry_index];
            let entry_matches = entry.section_id.matches(&dependencies[dependency_index]);
            let entry_decision = entry.decision();
            if entry_matches && !entry_decision.is_reuse() {
                return true;
            }
            entry_index += 1;
        }
        dependency_index += 1;
    }
    false
}
