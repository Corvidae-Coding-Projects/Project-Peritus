//! User-visible terminal disposition derived by the settlement reducer.

use vstd::prelude::*;

verus! {

/// Honest terminal state of one admitted coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RunDisposition {
    /// Current gates, public obligations, and independent review accepted the candidate.
    Accepted,
    /// Exact candidate work is available but strict acceptance is incomplete.
    CandidateAvailable,
    /// A material user answer is required before work can continue.
    WaitingForUser,
    /// The run stopped before producing any candidate.
    FailedNoCandidate,
    /// The user cancelled the run; an attached candidate remains unaccepted.
    Cancelled,
    /// Restart reconciliation is required; an attached candidate remains unaccepted.
    RecoveryRequired,
}

impl RunDisposition {
    /// Stable protocol tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Accepted => 1,
            Self::CandidateAvailable => 2,
            Self::WaitingForUser => 3,
            Self::FailedNoCandidate => 4,
            Self::Cancelled => 5,
            Self::RecoveryRequired => 6,
        }
    }

    /// Decodes a stable protocol tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Accepted),
            2 => Some(Self::CandidateAvailable),
            3 => Some(Self::WaitingForUser),
            4 => Some(Self::FailedNoCandidate),
            5 => Some(Self::Cancelled),
            6 => Some(Self::RecoveryRequired),
            _ => None,
        }
    }

    /// Whether strict automated qualification accepted the candidate.
    #[must_use]
    pub const fn is_accepted(self) -> bool { matches!(self, Self::Accepted) }
}

} // verus!
