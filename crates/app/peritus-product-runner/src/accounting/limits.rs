//! Exact caller-selected elapsed-budget evaluation.

use vstd::prelude::*;

verus! {

/// Exhausted caller-selected product-run budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetViolation {
    /// The configured elapsed horizon was exceeded.
    Elapsed,
}

impl BudgetViolation {
    pub(crate) closed spec fn spec_from_elapsed(elapsed_exceeded: bool) -> Option<Self> {
        if elapsed_exceeded { Some(Self::Elapsed) } else { None }
    }

    /// Reports only an explicitly configured elapsed horizon.
    pub(crate) const fn from_elapsed(elapsed_exceeded: bool) -> (violation: Option<Self>)
        ensures violation == Self::spec_from_elapsed(elapsed_exceeded),
    {
        if elapsed_exceeded { Some(Self::Elapsed) } else { None }
    }
}

} // verus!
