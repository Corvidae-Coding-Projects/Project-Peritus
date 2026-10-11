//! User-visible terminal disposition derived by the settlement reducer.

#[doc = " Honest terminal state of one admitted coding run."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RunDisposition {
    #[doc = " Current gates, public obligations, and independent review accepted the candidate."]
    Accepted,
    #[doc = " Exact candidate work is available but strict acceptance is incomplete."]
    CandidateAvailable,
    #[doc = " A material user answer is required before work can continue."]
    WaitingForUser,
    #[doc = " The run stopped before producing any candidate."]
    FailedNoCandidate,
    #[doc = " The user cancelled the run; an attached candidate remains unaccepted."]
    Cancelled,
    #[doc = " Restart reconciliation is required; an attached candidate remains unaccepted."]
    RecoveryRequired,
}
impl RunDisposition {
    #[doc = " Stable protocol tag."]
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
    #[doc = " Decodes a stable protocol tag."]
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
    #[doc = " Whether strict automated qualification accepted the candidate."]
    #[must_use]
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }
}
