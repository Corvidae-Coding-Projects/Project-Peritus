//! Pure fail-closed invalidation and reuse planning.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

mod evaluation;

use crate::{InvalidationRequest, KnowledgeError, KnowledgeSectionId, RunKnowledgeSnapshot};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;

#[doc = " Why a prior section may not be reused."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvalidationReason {
    #[doc = " Current observations belong to another run or workspace."]
    CandidateLineageChanged,
    #[doc = " The section belongs to another role view."]
    RoleIsolation,
    #[doc = " The section was created after the current checkpoint."]
    FutureObservation,
    #[doc = " At least one exact source digest is absent or changed."]
    SourceChanged,
    #[doc = " A named public clarification affects the requirement or design section."]
    UserClarification,
    #[doc = " A conversation-sensitive section predates an unscoped conversation revision."]
    ConversationRevisionChanged,
    #[doc = " Exact candidate content or incorporated conversation changed."]
    CandidateRevisionChanged,
    #[doc = " A direct dependency was invalidated."]
    DependencyInvalidated,
}
#[doc = " Reuse decision for one stable section identity."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReuseDecision {
    #[doc = " Exact prior knowledge remains current."]
    Reuse,
    #[doc = " Prior knowledge is retained only as stale diagnostic history."]
    Invalidate(#[doc = " Exact fail-closed cause selected by the planner."] InvalidationReason),
}
impl ReuseDecision {
    #[doc = " Whether this decision permits exact prior knowledge reuse."]
    #[must_use]
    pub const fn is_reuse(self) -> bool {
        matches!(self, Self::Reuse)
    }
}
#[doc = " One canonically ordered invalidation-plan entry."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlannedKnowledge {
    section_id: KnowledgeSectionId,
    decision: ReuseDecision,
}
impl PlannedKnowledge {
    const fn new(section_id: KnowledgeSectionId, decision: ReuseDecision) -> Self {
        Self { section_id, decision }
    }
    #[doc = " Stable section identity."]
    #[must_use]
    pub const fn section_id(self) -> KnowledgeSectionId {
        self.section_id
    }
    #[doc = " Current reuse decision."]
    #[must_use]
    pub const fn decision(self) -> ReuseDecision {
        self.decision
    }
}
#[doc = " Observable reuse and invalidation counts."]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReuseAccounting {
    total: usize,
    reused: usize,
    invalidated: usize,
}
impl ReuseAccounting {
    #[doc = " Number of sections considered."]
    #[must_use]
    pub const fn total(self) -> usize {
        self.total
    }
    #[doc = " Number of sections reused without rereading or recomputation."]
    #[must_use]
    pub const fn reused(self) -> usize {
        self.reused
    }
    #[doc = " Number of sections requiring refresh."]
    #[must_use]
    pub const fn invalidated(self) -> usize {
        self.invalidated
    }
}
#[doc = " Complete deterministic invalidation result for one role snapshot."]
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidationPlan {
    role: HarnessRole,
    candidate: CandidateIdentity,
    entries: Vec<PlannedKnowledge>,
    accounting: ReuseAccounting,
}
impl InvalidationPlan {
    #[doc = " Role whose retained context was evaluated."]
    #[must_use]
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    #[doc = " Current candidate observation used by the planner."]
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity {
        &self.candidate
    }
    #[doc = " Canonical per-section decisions."]
    #[must_use]
    pub const fn entries(&self) -> &[PlannedKnowledge] {
        self.entries.as_slice()
    }
    #[doc = " Reuse and invalidation counts."]
    #[must_use]
    pub const fn accounting(&self) -> ReuseAccounting {
        self.accounting
    }
    #[doc = " Whether one exact prior section remains reusable."]
    #[must_use]
    pub fn is_reused(&self, id: KnowledgeSectionId) -> bool {
        let mut index = 0;
        while index < self.entries.len() {
            let entry = self.entries[index];
            let entry_matches = entry.section_id.matches(&id);
            let entry_decision = entry.decision();
            if entry_matches && entry_decision.is_reuse() {
                return true;
            }
            index += 1;
        }
        false
    }
}
impl Clone for InvalidationPlan {
    fn clone(&self) -> Self {
        Self {
            role: self.role,
            candidate: self.candidate,
            entries: self.entries.clone(),
            accounting: self.accounting,
        }
    }
}
#[doc = " Computes current/stale decisions without performing any effect."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Rejects clarification targets that are absent or not requirement/design sections."]
pub fn plan_invalidation(
    snapshot: &RunKnowledgeSnapshot,
    request: &InvalidationRequest,
) -> Result<InvalidationPlan, KnowledgeError> {
    evaluation::validate_clarification_targets(snapshot, request)?;
    let sections = snapshot.sections();
    let mut entries = Vec::with_capacity(sections.len());
    let mut reused = 0usize;
    let mut invalidated = 0usize;
    let mut index = 0;
    while index < sections.len() {
        let section = &sections[index];
        let direct = evaluation::direct_decision(snapshot, section, request);
        let dependency_invalidated = if direct.is_reuse() {
            evaluation::dependency_was_invalidated(section, entries.as_slice())
        } else {
            false
        };
        let decision = if direct.is_reuse() && dependency_invalidated {
            ReuseDecision::Invalidate(InvalidationReason::DependencyInvalidated)
        } else {
            direct
        };
        if decision.is_reuse() {
            reused += 1;
        } else {
            invalidated += 1;
        }
        let entry = PlannedKnowledge::new(section.id(), decision);
        entries.push(entry);
        index += 1;
    }
    let plan = InvalidationPlan {
        role: snapshot.role(),
        candidate: *request.state().candidate(),
        entries,
        accounting: ReuseAccounting { total: sections.len(), reused, invalidated },
    };
    Ok(plan)
}
