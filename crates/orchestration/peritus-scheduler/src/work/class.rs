//! Closed execution-class vocabulary used by work and worker selection.

use vstd::prelude::*;

verus! {

/// Closed supported execution class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionClass {
    /// Model inference or agent turn.
    Model,
    /// Inert tool request handled by a governed router.
    Tool,
    /// Acceptance-gate execution.
    Gate,
    /// Independent review work.
    Review,
    /// Orchestration/control-plane work.
    Coordination,
}

impl ExecutionClass {
    /// Compares exact execution classes for verified selector lookup.
    pub(crate) const fn same(self, other: Self) -> (result: bool)
        ensures result == (self == other),
    {
        matches!(
            (self, other),
            (Self::Model, Self::Model)
                | (Self::Tool, Self::Tool)
                | (Self::Gate, Self::Gate)
                | (Self::Review, Self::Review)
                | (Self::Coordination, Self::Coordination)
        )
    }
}

} // verus!
