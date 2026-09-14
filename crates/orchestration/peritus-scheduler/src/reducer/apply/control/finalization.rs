//! Exact finalization admission, summary planning, and terminal commit.
//!
//! Canonical SHA-256 remains an ordinary-code boundary between `prepare` and `commit`.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

#[cfg(verus_only)]
use crate::SchedulerPhase;
use crate::{SchedulerEventKind, SchedulerState, SchedulerTerminal, WorkPhase};

verus! {

/// Exact finalization rejection before any state mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizationRejection {
    /// At least one work item is nonterminal or live ownership remains.
    NonterminalWorkOrReservations,
}

/// Opaque exact terminal summary prepared before ordinary canonical hashing.
pub struct FinalizationPlan {
    terminal: SchedulerTerminal,
}

impl FinalizationPlan {
    /// Returns the mathematical zero-digest terminal summary carried by this plan.
    pub closed spec fn spec_terminal(&self) -> SchedulerTerminal { self.terminal }

    /// The plan exactly classifies the supplied work sequence.
    pub open spec fn matches(&self, work: Seq<crate::WorkRecord>) -> bool {
        SchedulerTerminal::summary_matches(
            work,
            self.spec_terminal().spec_kind(),
            self.spec_terminal().spec_non_successful_work(),
        )
    }

    /// Borrows the canonical terminal fields to hash in ordinary code.
    pub const fn digest_input(&self) -> (result: &SchedulerTerminal)
        ensures *result == self.spec_terminal(),
    {
        &self.terminal
    }

    /// Installs the ordinary supplied digest without changing classified fields.
    fn finish(self, supplied_digest: Sha256Digest) -> (result: SchedulerTerminal)
        ensures
            result.spec_kind() == self.spec_terminal().spec_kind(),
            result.spec_non_successful_work()
                == self.spec_terminal().spec_non_successful_work(),
            result.spec_digest() == supplied_digest,
    {
        self.terminal.with_digest(supplied_digest)
    }
}

/// Every retained work item has reached the terminal lifecycle.
pub open spec fn every_work_terminal(state: &SchedulerState) -> bool {
    forall |index: int| 0 <= index < state.spec_work().len() ==>
        state.spec_work()[index].spec_phase() == WorkPhase::Terminal
}

/// Exact existing domain admission for finalization.
pub open spec fn finalization_is_admitted(state: &SchedulerState) -> bool {
    every_work_terminal(state) && state.spec_reservations().len() == 0
}

/// Every authoritative state field outside phase and terminal summary is unchanged.
pub open spec fn finalization_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_work() == before.spec_work()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
}

/// Exact result of the read-only admission and summary pass.
pub open spec fn preparation_matches(
    state: &SchedulerState,
    result: &Result<FinalizationPlan, FinalizationRejection>,
) -> bool {
    match result {
        Ok(plan) => finalization_is_admitted(state) && plan.matches(state.spec_work()),
        Err(FinalizationRejection::NonterminalWorkOrReservations) => {
            !finalization_is_admitted(state)
        },
    }
}

/// Exact terminal payload, state update, and full frame produced from one plan.
pub open spec fn commit_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    plan: &FinalizationPlan,
    supplied_digest: Sha256Digest,
    event: &SchedulerEventKind,
) -> bool {
    match event {
        SchedulerEventKind::SchedulerFinalized { terminal } => {
            &&& terminal.spec_kind() == plan.spec_terminal().spec_kind()
            &&& terminal.spec_non_successful_work()
                == plan.spec_terminal().spec_non_successful_work()
            &&& terminal.spec_digest() == supplied_digest
            &&& after.spec_phase() == SchedulerPhase::Terminal
            &&& match after.spec_terminal() {
                Some(stored) => SchedulerTerminal::clone_equivalent(terminal, &stored),
                None => false,
            }
            &&& finalization_preserves_other_state(before, after)
            &&& (plan.matches(before.spec_work()) ==>
                SchedulerTerminal::evaluation_matches(
                    before.spec_work(), supplied_digest, terminal,
                ))
        },
        _ => false,
    }
}

/// Checks the established finalization guard and summarizes admitted work exactly once.
pub(super) fn prepare(
    state: &SchedulerState,
) -> (result: Result<FinalizationPlan, FinalizationRejection>)
    ensures preparation_matches(state, &result),
{
    let work = state.work();
    let mut index = 0;
    while index < work.len()
        invariant
            index <= work.len(),
            work@ == state.spec_work(),
            forall |prior: int| 0 <= prior < index ==>
                work@[prior].spec_phase() == WorkPhase::Terminal,
        decreases work.len() - index,
    {
        if !work[index].phase().same(WorkPhase::Terminal) {
            proof {
                assert(0 <= (index as int) < state.spec_work().len());
                assert(state.spec_work()[index as int].spec_phase() != WorkPhase::Terminal);
                assert(!every_work_terminal(state)) by {
                    reveal(every_work_terminal);
                }
                reveal(preparation_matches);
                reveal(finalization_is_admitted);
                reveal(every_work_terminal);
            }
            return Err(FinalizationRejection::NonterminalWorkOrReservations);
        }
        index += 1;
    }
    if !state.reservations().is_empty() {
        proof {
            reveal(preparation_matches);
            reveal(finalization_is_admitted);
            reveal(every_work_terminal);
        }
        return Err(FinalizationRejection::NonterminalWorkOrReservations);
    }
    let terminal = SchedulerTerminal::evaluate_with_digest(
        state.work(),
        Sha256Digest::new([0; 32]),
    );
    let result = Ok(FinalizationPlan { terminal });
    proof {
        reveal(preparation_matches);
        reveal(finalization_is_admitted);
        reveal(every_work_terminal);
        reveal(FinalizationPlan::matches);
    }
    result
}

/// Commits one prepared summary after ordinary code supplies its canonical digest.
pub(super) fn commit(
    state: &mut SchedulerState,
    plan: FinalizationPlan,
    supplied_digest: Sha256Digest,
) -> (event: SchedulerEventKind)
    ensures
        commit_matches(old(state), final(state), &plan, supplied_digest, &event),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered()
            ==> final(state).spec_collections_ordered(),
{
    let terminal = plan.finish(supplied_digest);
    let stored = terminal.clone();
    crate::state::mutation::set_terminal(state, stored);
    let event = SchedulerEventKind::SchedulerFinalized { terminal };
    proof {
        reveal(commit_matches);
        reveal(finalization_preserves_other_state);
        reveal(FinalizationPlan::matches);
        reveal(SchedulerTerminal::evaluation_matches);
    }
    event
}

} // verus!
