//! Applied and rejected reducer outcomes.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{KernelAggregate, KernelError, KernelEvent};

#[doc = " Result of evaluating current acceptance evidence."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AcceptanceOutcome {
    #[doc = " The exact current contract was satisfied."]
    Accepted,
    #[doc = " Work remains before acceptance can succeed."]
    NeedsChanges {
        #[doc = " Number of typed unmet conditions reported by B2."]
        unmet_conditions: usize,
    },
}
#[doc = " One accepted logical next-state/event plan."]
#[derive(Debug, Eq, PartialEq)]
pub struct KernelTransition {
    pub(crate) aggregate: KernelAggregate,
    pub(crate) event: KernelEvent,
    pub(crate) acceptance_outcome: Option<AcceptanceOutcome>,
}
impl KernelTransition {
    pub(crate) const fn new(
        aggregate: KernelAggregate,
        event: KernelEvent,
        acceptance_outcome: Option<AcceptanceOutcome>,
    ) -> Self {
        Self { aggregate, event, acceptance_outcome }
    }
    #[doc = " Borrows the exact next aggregate."]
    #[must_use]
    pub const fn aggregate(&self) -> &KernelAggregate {
        &self.aggregate
    }
    #[doc = " Returns the emitted event plan."]
    #[must_use]
    pub const fn event(&self) -> KernelEvent {
        self.event
    }
    #[doc = " Returns an acceptance result only for evaluation commands."]
    #[must_use]
    pub const fn acceptance_outcome(&self) -> Option<AcceptanceOutcome> {
        self.acceptance_outcome
    }
    #[doc = " Consumes the plan into its next aggregate and event."]
    #[must_use]
    pub fn into_parts(self) -> (KernelAggregate, KernelEvent, Option<AcceptanceOutcome>) {
        (self.aggregate, self.event, self.acceptance_outcome)
    }
}
#[doc = " Total reducer result; rejection returns the unchanged owned aggregate."]
#[derive(Debug, Eq, PartialEq)]
pub enum KernelOutcome {
    #[doc = " The command produced one next-state/event plan."]
    Applied(KernelTransition),
    #[doc = " The command was rejected and the original aggregate is returned."]
    Rejected {
        #[doc = " Unchanged input aggregate."]
        aggregate: KernelAggregate,
        #[doc = " Exact deterministic rejection."]
        error: KernelError,
    },
}
impl KernelOutcome {
    #[doc = " Returns whether the command was accepted."]
    #[must_use]
    pub const fn is_applied(&self) -> bool {
        matches!(self, Self::Applied(_))
    }
    #[doc = " Consumes the outcome into a conventional result while preserving rejected state."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the unchanged aggregate and deterministic error when reduction was rejected."]
    #[allow(
        clippy::result_large_err,
        reason = "rejections intentionally return ownership of the unchanged aggregate"
    )]
    pub fn into_result(self) -> Result<KernelTransition, (KernelAggregate, KernelError)> {
        match self {
            Self::Applied(transition) => Ok(transition),
            Self::Rejected { aggregate, error } => Err((aggregate, error)),
        }
    }
}
