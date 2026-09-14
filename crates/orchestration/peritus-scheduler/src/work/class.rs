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
    /// Returns the independent canonical rank used for worker admission.
    pub closed spec fn spec_rank(&self) -> u8 {
        match self {
            Self::Model => 0,
            Self::Tool => 1,
            Self::Gate => 2,
            Self::Review => 3,
            Self::Coordination => 4,
        }
    }

    /// Returns whether this class strictly precedes another canonical class.
    pub open spec fn spec_precedes(&self, other: &Self) -> bool {
        self.spec_rank() < other.spec_rank()
    }

    /// Compares canonical execution-class ranks for checked worker admission.
    pub(crate) const fn precedes(self, other: Self) -> (result: bool)
        ensures result == self.spec_precedes(&other),
    {
        let left = match self {
            Self::Model => 0u8,
            Self::Tool => 1u8,
            Self::Gate => 2u8,
            Self::Review => 3u8,
            Self::Coordination => 4u8,
        };
        let right = match other {
            Self::Model => 0u8,
            Self::Tool => 1u8,
            Self::Gate => 2u8,
            Self::Review => 3u8,
            Self::Coordination => 4u8,
        };
        left < right
    }

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
