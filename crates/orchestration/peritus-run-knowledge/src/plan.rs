//! Pure fail-closed invalidation and reuse planning.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

mod evaluation;

use crate::{InvalidationRequest, KnowledgeError, KnowledgeSectionId, RunKnowledgeSnapshot};
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

impl ReuseDecision {
    /// Logical classification of an exact reuse decision.
    pub open spec fn spec_is_reuse(self) -> bool { self == Self::Reuse }

    /// Whether this decision permits exact prior knowledge reuse.
    #[must_use]
    pub const fn is_reuse(self) -> (reuse: bool)
        ensures reuse == self.spec_is_reuse(),
    {
        matches!(self, Self::Reuse)
    }
}

/// One canonically ordered invalidation-plan entry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlannedKnowledge {
    section_id: KnowledgeSectionId,
    decision: ReuseDecision,
}

impl PlannedKnowledge {
    /// Logical view of the stable section identity.
    pub closed spec fn spec_section_id(&self) -> KnowledgeSectionId { self.section_id }

    /// Logical view of the exact reuse decision.
    pub closed spec fn spec_decision(&self) -> ReuseDecision { self.decision }

    const fn new(
        section_id: KnowledgeSectionId,
        decision: ReuseDecision,
    ) -> (result: Self)
        ensures
            result.spec_section_id() == section_id,
            result.spec_decision() == decision,
    {
        Self { section_id, decision }
    }

    /// Stable section identity.
    #[must_use]
    pub const fn section_id(self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_section_id(),
    { self.section_id }

    /// Current reuse decision.
    #[must_use]
    pub const fn decision(self) -> (decision: ReuseDecision)
        ensures decision == self.spec_decision(),
    { self.decision }
}

/// Observable reuse and invalidation counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReuseAccounting {
    total: usize,
    reused: usize,
    invalidated: usize,
}

impl ReuseAccounting {
    /// Logical number of sections considered.
    pub closed spec fn spec_total(&self) -> nat { self.total as nat }

    /// Logical number of reused sections.
    pub closed spec fn spec_reused(&self) -> nat { self.reused as nat }

    /// Logical number of invalidated sections.
    pub closed spec fn spec_invalidated(&self) -> nat { self.invalidated as nat }

    /// Number of sections considered.
    #[must_use]
    pub const fn total(self) -> (total: usize)
        ensures total as nat == self.spec_total(),
    { self.total }

    /// Number of sections reused without rereading or recomputation.
    #[must_use]
    pub const fn reused(self) -> (reused: usize)
        ensures reused as nat == self.spec_reused(),
    { self.reused }

    /// Number of sections requiring refresh.
    #[must_use]
    pub const fn invalidated(self) -> (invalidated: usize)
        ensures invalidated as nat == self.spec_invalidated(),
    { self.invalidated }
}

/// Complete deterministic invalidation result for one role snapshot.
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidationPlan {
    role: HarnessRole,
    candidate: CandidateIdentity,
    entries: Vec<PlannedKnowledge>,
    accounting: ReuseAccounting,
}

impl InvalidationPlan {
    /// Logical role whose sections were evaluated.
    pub closed spec fn spec_role(&self) -> HarnessRole { self.role }

    /// Logical current candidate used by the planner.
    pub closed spec fn spec_candidate(&self) -> CandidateIdentity { self.candidate }

    /// Logical ordered plan entries.
    pub closed spec fn spec_entries(&self) -> Seq<PlannedKnowledge> { self.entries@ }

    /// Logical accounting values.
    pub closed spec fn spec_accounting(&self) -> ReuseAccounting { self.accounting }

    /// Exact successful planner output for the supplied snapshot and request.
    pub open spec fn spec_refines(
        &self,
        snapshot: &RunKnowledgeSnapshot,
        request: &InvalidationRequest,
    ) -> bool {
        &&& snapshot.spec_topology_valid()
        &&& self.spec_role() == snapshot.spec_role()
        &&& crate::model::candidates_match(
            &self.spec_candidate(), &request.spec_state().spec_candidate())
        &&& self.spec_entries().len() == snapshot.spec_sections().len()
        &&& crate::model::entries_are_exact_prefix(snapshot, request, self.spec_entries())
        &&& self.spec_accounting().spec_total() == self.spec_entries().len()
        &&& self.spec_accounting().spec_reused()
            == crate::model::reused_count(self.spec_entries())
        &&& self.spec_accounting().spec_invalidated()
            == crate::model::invalidated_count(self.spec_entries())
        &&& self.spec_accounting().spec_reused()
            + self.spec_accounting().spec_invalidated() == self.spec_entries().len()
        &&& crate::model::transitive_invalidation_closed(
            snapshot.spec_sections(), self.spec_entries())
    }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_role() == right.spec_role()
            && left.spec_candidate() == right.spec_candidate()
            && left.spec_entries() == right.spec_entries()
            && left.spec_accounting() == right.spec_accounting()
    }

    /// Role whose retained context was evaluated.
    #[must_use]
    pub const fn role(&self) -> (role: HarnessRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Current candidate observation used by the planner.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: &CandidateIdentity)
        ensures *candidate == self.spec_candidate(),
    { &self.candidate }

    /// Canonical per-section decisions.
    #[must_use]
    pub const fn entries(&self) -> (entries: &[PlannedKnowledge])
        ensures entries@ == self.spec_entries(),
    { self.entries.as_slice() }

    /// Reuse and invalidation counts.
    #[must_use]
    pub const fn accounting(&self) -> (accounting: ReuseAccounting)
        ensures accounting == self.spec_accounting(),
    { self.accounting }

    /// Whether one exact prior section remains reusable.
    #[must_use]
    pub fn is_reused(&self, id: KnowledgeSectionId) -> (reused: bool)
        ensures reused == crate::model::plan_reuses(self.spec_entries(), id),
    {
        let mut index = 0;
        while index < self.entries.len()
            invariant
                index <= self.entries.len(),
                forall |prior: int| 0 <= prior < index ==>
                    !(self.entries@[prior].spec_section_id().spec_matches(&id)
                        && self.entries@[prior].spec_decision().spec_is_reuse()),
            decreases self.entries.len() - index,
        {
            let entry = self.entries[index];
            let entry_matches = entry.section_id.matches(&id);
            let entry_decision = entry.decision();
            if entry_matches && entry_decision.is_reuse()
            {
                assert(crate::model::plan_reuses(self.spec_entries(), id)) by {
                    let witness = index as int;
                    assert(self.spec_entries()[witness].spec_section_id().spec_matches(&id));
                    assert(self.spec_entries()[witness].spec_decision().spec_is_reuse());
                }
                return true;
            }
            assert(!(self.entries@[index as int].spec_section_id().spec_matches(&id)
                && self.entries@[index as int].spec_decision().spec_is_reuse())) by {
                assert(entry_decision.spec_is_reuse()
                    == self.entries@[index as int].spec_decision().spec_is_reuse());
            }
            index += 1;
        }
        false
    }
}

impl Clone for InvalidationPlan {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            role: self.role,
            candidate: self.candidate,
            entries: self.entries.clone(),
            accounting: self.accounting,
        }
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
) -> (result: Result<InvalidationPlan, KnowledgeError>)
    ensures
        result.is_ok() == crate::model::clarification_targets_valid(snapshot, request),
        match result {
            Ok(plan) => plan.spec_refines(snapshot, request),
            Err(error) => crate::model::clarification_targets_error(snapshot, request, error),
        },
{
    proof {
        use_type_invariant(&*snapshot);
    }
    evaluation::validate_clarification_targets(snapshot, request)?;
    let sections = snapshot.sections();
    let mut entries = Vec::with_capacity(sections.len());
    let mut reused = 0usize;
    let mut invalidated = 0usize;
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            sections@ == snapshot.spec_sections(),
            entries.len() == index,
            crate::model::entries_are_exact_prefix(snapshot, request, entries@),
            reused as nat == crate::model::reused_count(entries@),
            invalidated as nat == crate::model::invalidated_count(entries@),
            reused + invalidated == index,
        decreases sections.len() - index,
    {
        let section = &sections[index];
        let ghost prior_entries = entries@;
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
        assert(decision == crate::model::planned_decision(
            snapshot, section, request, prior_entries));
        if decision.is_reuse() {
            reused += 1;
        } else {
            invalidated += 1;
        }
        let entry = PlannedKnowledge::new(section.id(), decision);
        entries.push(entry);
        assert(entries@ == prior_entries.push(entry));
        proof {
            crate::model::counts_after_push(prior_entries, entry);
        }
        assert(entries@.take(index as int) == prior_entries);
        assert((index as int) < snapshot.spec_sections().len());
        assert(sections@[index as int] == snapshot.spec_sections()[index as int]);
        assert(*section == snapshot.spec_sections()[index as int]);
        assert(crate::model::entries_are_exact_prefix(snapshot, request, entries@)) by {
            assert forall |entry_index: int| 0 <= entry_index < entries@.len() implies
                entries@[entry_index].spec_section_id()
                    .spec_matches(&snapshot.spec_sections()[entry_index].spec_id())
                && entries@[entry_index].spec_decision() == crate::model::planned_decision(
                    snapshot,
                    &snapshot.spec_sections()[entry_index],
                    request,
                    entries@.take(entry_index),
                ) by {
                if entry_index < index {
                    assert(entries@[entry_index] == prior_entries[entry_index]);
                    assert(entries@.take(entry_index) == prior_entries.take(entry_index));
                } else {
                    assert(entry_index == index);
                    assert(entries@[entry_index] == entry);
                    assert(entry.spec_section_id().spec_matches(&section.spec_id()));
                }
            }
        }
        index += 1;
    }
    proof {
        crate::model::exact_plan_is_transitively_closed(snapshot, request, entries@);
    }
    let plan = InvalidationPlan {
        role: snapshot.role(),
        candidate: *request.state().candidate(),
        entries,
        accounting: ReuseAccounting { total: sections.len(), reused, invalidated },
    };
    assert(plan.spec_refines(snapshot, request)) by {
        assert(snapshot.spec_topology_valid());
        assert(plan.spec_entries().len() == snapshot.spec_sections().len());
        assert(crate::model::entries_are_exact_prefix(
            snapshot, request, plan.spec_entries()));
        assert(crate::model::transitive_invalidation_closed(
            snapshot.spec_sections(), plan.spec_entries()));
    }
    Ok(plan)
}


} // verus!
