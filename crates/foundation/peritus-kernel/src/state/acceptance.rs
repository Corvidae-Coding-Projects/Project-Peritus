//! Acceptance lifecycle state.

use vstd::prelude::*;

verus! {

/// Lifecycle of acceptance evaluation for the current run revision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AcceptancePhase {
    /// No evaluation is active for the current candidate.
    Pending,
    /// Acceptance evaluation was explicitly begun.
    Evaluating,
    /// Evaluation found unmet conditions and the candidate requires work.
    NeedsChanges,
    /// The exact current evidence satisfied the exact current contract.
    Accepted,
    /// The run terminated without acceptance.
    Terminated,
}

impl AcceptancePhase {
    /// Returns whether this phase denotes accepted work.
    #[must_use]
    pub const fn is_accepted(self) -> bool { matches!(self, Self::Accepted) }
}

} // verus!
