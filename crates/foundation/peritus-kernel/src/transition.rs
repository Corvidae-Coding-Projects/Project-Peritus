//! Applied and rejected reducer outcomes.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{KernelAggregate, KernelError, KernelEvent};
use vstd::prelude::*;

verus! {

/// Result of evaluating current acceptance evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AcceptanceOutcome {
    /// The exact current contract was satisfied.
    Accepted,
    /// Work remains before acceptance can succeed.
    NeedsChanges {
        /// Number of typed unmet conditions reported by B2.
        unmet_conditions: usize,
    },
}

/// One accepted logical next-state/event plan.
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
    ) -> (result: Self)
        ensures
            result.aggregate == aggregate,
            result.event == event,
            result.acceptance_outcome == acceptance_outcome,
            result.aggregate.head_event_id == aggregate.head_event_id,
            result.aggregate.last_sequence == aggregate.last_sequence,
            result.aggregate.revision == aggregate.revision,
            result.event.id == event.id,
            result.event.command_id == event.command_id,
            result.event.sequence == event.sequence,
            result.event.previous_event_id == event.previous_event_id,
            result.event.revision == event.revision,
            result.event.kind == event.kind,
            result.event.subject == event.subject,
    {
        Self { aggregate, event, acceptance_outcome }
    }
    /// Borrows the exact next aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &KernelAggregate { &self.aggregate }
    /// Returns the emitted event plan.
    #[must_use]
    pub const fn event(&self) -> KernelEvent { self.event }
    /// Returns an acceptance result only for evaluation commands.
    #[must_use]
    pub const fn acceptance_outcome(&self) -> Option<AcceptanceOutcome> {
        self.acceptance_outcome
    }
    /// Consumes the plan into its next aggregate and event.
    #[must_use]
    pub fn into_parts(self) -> (KernelAggregate, KernelEvent, Option<AcceptanceOutcome>) {
        (self.aggregate, self.event, self.acceptance_outcome)
    }
}

/// Total reducer result; rejection returns the unchanged owned aggregate.
#[derive(Debug, Eq, PartialEq)]
pub enum KernelOutcome {
    /// The command produced one next-state/event plan.
    Applied(KernelTransition),
    /// The command was rejected and the original aggregate is returned.
    Rejected {
        /// Unchanged input aggregate.
        aggregate: KernelAggregate,
        /// Exact deterministic rejection.
        error: KernelError,
    },
}

impl KernelOutcome {
    /// Returns whether the command was accepted.
    #[must_use]
    pub const fn is_applied(&self) -> bool { matches!(self, Self::Applied(_)) }
    /// Consumes the outcome into a conventional result while preserving rejected state.
    ///
    /// # Errors
    ///
    /// Returns the unchanged aggregate and deterministic error when reduction was rejected.
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

} // verus!
