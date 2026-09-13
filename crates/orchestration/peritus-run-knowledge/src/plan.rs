//! Pure fail-closed invalidation and reuse planning.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    InvalidationRequest, KnowledgeChange, KnowledgeError, KnowledgeErrorKind, KnowledgeSection,
    KnowledgeSectionId, KnowledgeSectionKind, RunKnowledgeSnapshot,
};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Why a prior section may not be reused.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvalidationReason {
    /// Current observations belong to another run or workspace.
    CandidateLineageChanged,
    /// The section belongs to another role view.
    RoleIsolation,
    /// The section was created after the current checkpoint.
    FutureObservation,
    /// At least one exact source digest is absent or changed.
    SourceChanged,
    /// A named public clarification affects the requirement or design section.
    UserClarification,
    /// A conversation-sensitive section predates an unscoped conversation revision.
    ConversationRevisionChanged,
    /// Exact candidate content or incorporated conversation changed.
    CandidateRevisionChanged,
    /// A direct dependency was invalidated.
    DependencyInvalidated,
}

/// Reuse decision for one stable section identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReuseDecision {
    /// Exact prior knowledge remains current.
    Reuse,
    /// Prior knowledge is retained only as stale diagnostic history.
    Invalidate(
        /// Exact fail-closed cause selected by the planner.
        InvalidationReason,
    ),
}

/// One canonically ordered invalidation-plan entry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlannedKnowledge {
    section_id: KnowledgeSectionId,
    decision: ReuseDecision,
}

impl PlannedKnowledge {
    const fn new(section_id: KnowledgeSectionId, decision: ReuseDecision) -> Self {
        Self { section_id, decision }
    }

    /// Stable section identity.
    #[must_use]
    pub const fn section_id(self) -> KnowledgeSectionId { self.section_id }

    /// Current reuse decision.
    #[must_use]
    pub const fn decision(self) -> ReuseDecision { self.decision }
}

/// Observable reuse and invalidation counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReuseAccounting {
    total: usize,
    reused: usize,
    invalidated: usize,
}

impl ReuseAccounting {
    /// Number of sections considered.
    #[must_use]
    pub const fn total(self) -> usize { self.total }

    /// Number of sections reused without rereading or recomputation.
    #[must_use]
    pub const fn reused(self) -> usize { self.reused }

    /// Number of sections requiring refresh.
    #[must_use]
    pub const fn invalidated(self) -> usize { self.invalidated }
}

/// Complete deterministic invalidation result for one role snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationPlan {
    role: HarnessRole,
    candidate: CandidateIdentity,
    entries: Vec<PlannedKnowledge>,
    accounting: ReuseAccounting,
}

impl InvalidationPlan {
    /// Role whose retained context was evaluated.
    #[must_use]
    pub const fn role(&self) -> HarnessRole { self.role }

    /// Current candidate observation used by the planner.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity { &self.candidate }

    /// Canonical per-section decisions.
    #[must_use]
    pub const fn entries(&self) -> &[PlannedKnowledge] { self.entries.as_slice() }

    /// Reuse and invalidation counts.
    #[must_use]
    pub const fn accounting(&self) -> ReuseAccounting { self.accounting }

    /// Whether one exact prior section remains reusable.
    #[must_use]
    pub fn is_reused(&self, id: KnowledgeSectionId) -> bool {
        let mut index = 0;
        while index < self.entries.len()
            invariant index <= self.entries.len(),
            decreases self.entries.len() - index,
        {
            if self.entries[index].section_id == id {
                return self.entries[index].decision == ReuseDecision::Reuse;
            }
            if self.entries[index].section_id > id {
                return false;
            }
            index += 1;
        }
        false
    }
}

/// Computes current/stale decisions without performing any effect.
///
/// # Errors
///
/// Rejects clarification targets that are absent or not requirement/design sections.
pub fn plan_invalidation(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<InvalidationPlan, KnowledgeError> {
    validate_clarification_targets(snapshot, request)?;
    let sections = snapshot.sections();
    let mut entries = Vec::with_capacity(sections.len());
    let mut reused = 0usize;
    let mut invalidated = 0usize;
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            entries.len() == index,
            reused <= index,
            invalidated <= index,
        decreases sections.len() - index,
    {
        let section = &sections[index];
        let mut decision = direct_decision(snapshot, section, request);
        if decision == ReuseDecision::Reuse
            && dependency_was_invalidated(section, entries.as_slice())
        {
            decision = ReuseDecision::Invalidate(InvalidationReason::DependencyInvalidated);
        }
        if decision == ReuseDecision::Reuse {
            reused += 1;
        } else {
            invalidated += 1;
        }
        entries.push(PlannedKnowledge::new(section.id(), decision));
        index += 1;
    }
    Ok(InvalidationPlan {
        role: snapshot.role(),
        candidate: *request.state().candidate(),
        entries,
        accounting: ReuseAccounting { total: sections.len(), reused, invalidated },
    })
}

fn validate_clarification_targets(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<(), KnowledgeError> {
    if request.change() != KnowledgeChange::UserClarification {
        return Ok(());
    }
    let targets = request.affected_sections();
    let mut index = 0;
    while index < targets.len()
        invariant index <= targets.len(),
        decreases targets.len() - index,
    {
        let Some(section) = snapshot.section(targets[index]) else {
            return Err(KnowledgeError::section(
                KnowledgeErrorKind::InvalidClarificationTarget,
                targets[index],
            ));
        };
        if !matches!(
            section.kind(),
            KnowledgeSectionKind::LiteralRequirementLedger | KnowledgeSectionKind::DesignSection
        ) {
            return Err(KnowledgeError::section(
                KnowledgeErrorKind::InvalidClarificationTarget,
                targets[index],
            ));
        }
        index += 1;
    }
    Ok(())
}

fn direct_decision(
    snapshot: &RunKnowledgeSnapshot,
    section: &KnowledgeSection,
    request: &InvalidationRequest,
) -> ReuseDecision {
    let current = request.state().candidate();
    let binding = section.binding();
    if !binding.candidate().same_lineage(current) {
        return ReuseDecision::Invalidate(InvalidationReason::CandidateLineageChanged);
    }
    if binding.role() != snapshot.role() {
        return ReuseDecision::Invalidate(InvalidationReason::RoleIsolation);
    }
    if binding.creation_sequence() > current.checkpoint_sequence() {
        return ReuseDecision::Invalidate(InvalidationReason::FutureObservation);
    }
    if !all_sources_current(binding.sources(), request) {
        return ReuseDecision::Invalidate(InvalidationReason::SourceChanged);
    }
    if request.change() == KnowledgeChange::UserClarification && request.affects(section.id()) {
        return ReuseDecision::Invalidate(InvalidationReason::UserClarification);
    }
    let same_conversation = binding.candidate().conversation_revision()
        == current.conversation_revision();
    if section.kind().depends_on_conversation()
        && !same_conversation
        && request.change() != KnowledgeChange::UserClarification
    {
        return ReuseDecision::Invalidate(InvalidationReason::ConversationRevisionChanged);
    }
    let same_candidate = binding.candidate().same_candidate(current);
    if section.kind().depends_on_candidate() && !same_candidate {
        return ReuseDecision::Invalidate(InvalidationReason::CandidateRevisionChanged);
    }
    if !crate::verified::reuse_allowed(crate::ReusePremises::new(
        crate::ReusePremiseStatus::Satisfied,
        crate::ReusePremiseStatus::Satisfied,
        crate::ReusePremiseStatus::Satisfied,
        crate::ReusePremiseStatus::Satisfied,
        crate::ReusePremiseStatus::from_satisfied(
            !section.kind().depends_on_conversation()
                || same_conversation
                || request.change() == KnowledgeChange::UserClarification,
        ),
        crate::ReusePremiseStatus::Satisfied,
    )) {
        return ReuseDecision::Invalidate(InvalidationReason::CandidateRevisionChanged);
    }
    ReuseDecision::Reuse
}

fn all_sources_current(sources: &[crate::SourceDigest], request: &InvalidationRequest) -> bool {
    let mut index = 0;
    while index < sources.len()
        invariant index <= sources.len(),
        decreases sources.len() - index,
    {
        if !request.state().source_is_current(sources[index]) {
            return false;
        }
        index += 1;
    }
    true
}

fn dependency_was_invalidated(
    section: &KnowledgeSection,
    entries: &[PlannedKnowledge],
) -> bool {
    let dependencies = section.dependencies();
    let mut dependency_index = 0;
    while dependency_index < dependencies.len()
        invariant dependency_index <= dependencies.len(),
        decreases dependencies.len() - dependency_index,
    {
        let mut entry_index = 0;
        while entry_index < entries.len()
            invariant
                dependency_index < dependencies.len(),
                entry_index <= entries.len(),
            decreases entries.len() - entry_index,
        {
            if entries[entry_index].section_id == dependencies[dependency_index] {
                if entries[entry_index].decision != ReuseDecision::Reuse {
                    return true;
                }
                break;
            }
            entry_index += 1;
        }
        dependency_index += 1;
    }
    false
}

} // verus!
