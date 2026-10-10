//! Exact finalization admission, summary planning, and terminal commit.
//!
//! Canonical SHA-256 remains an ordinary-code boundary between `prepare` and `commit`.

use peritus_types::Sha256Digest;

use crate::{SchedulerEventKind, SchedulerState, SchedulerTerminal, WorkPhase};

#[doc = " Exact finalization rejection before any state mutation."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizationRejection {
    #[doc = " At least one work item is nonterminal or live ownership remains."]
    NonterminalWorkOrReservations,
}
#[doc = " Opaque exact terminal summary prepared before ordinary canonical hashing."]
pub struct FinalizationPlan {
    terminal: SchedulerTerminal,
}
impl FinalizationPlan {
    #[doc = " Borrows the canonical terminal fields to hash in ordinary code."]
    pub const fn digest_input(&self) -> &SchedulerTerminal {
        &self.terminal
    }
    #[doc = " Installs the ordinary supplied digest without changing classified fields."]
    fn finish(self, supplied_digest: Sha256Digest) -> SchedulerTerminal {
        self.terminal.with_digest(supplied_digest)
    }
}
#[doc = " Checks the established finalization guard and summarizes admitted work exactly once."]
pub(super) fn prepare(state: &SchedulerState) -> Result<FinalizationPlan, FinalizationRejection> {
    let work = state.work();
    let mut index = 0;
    while index < work.len() {
        if !work[index].phase().same(WorkPhase::Terminal) {
            return Err(FinalizationRejection::NonterminalWorkOrReservations);
        }
        index += 1;
    }
    if !state.reservations().is_empty() {
        return Err(FinalizationRejection::NonterminalWorkOrReservations);
    }
    let terminal =
        SchedulerTerminal::evaluate_with_digest(state.work(), Sha256Digest::new([0; 32]));
    let result = Ok(FinalizationPlan { terminal });
    result
}
#[doc = " Commits one prepared summary after ordinary code supplies its canonical digest."]
pub(super) fn commit(
    state: &mut SchedulerState,
    plan: FinalizationPlan,
    supplied_digest: Sha256Digest,
) -> SchedulerEventKind {
    let terminal = plan.finish(supplied_digest);
    let stored = terminal.clone();
    crate::state::mutation::set_terminal(state, stored);
    let event = SchedulerEventKind::SchedulerFinalized { terminal };
    event
}
