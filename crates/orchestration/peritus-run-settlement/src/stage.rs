//! Currently supported qualification stage for one exact candidate.

#[doc = " Qualification currently supported by observations for one exact candidate."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CandidateStage {
    #[doc = " Candidate content was observed."]
    Observed,
    #[doc = " Workspace content differs from the run baseline."]
    Changed,
    #[doc = " The producing role reported a self-check."]
    SelfChecked,
    #[doc = " Required deterministic gates passed for this candidate."]
    GatesPassed,
    #[doc = " Gates passed and independent review has not yet completed."]
    ReviewPending,
    #[doc = " Gates, public obligations, and independent review all passed."]
    Qualified,
}
impl CandidateStage {
    #[doc = " Stable protocol tag."]
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Observed => 1,
            Self::Changed => 2,
            Self::SelfChecked => 3,
            Self::GatesPassed => 4,
            Self::ReviewPending => 5,
            Self::Qualified => 6,
        }
    }
    #[doc = " Decodes a stable protocol tag."]
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Observed),
            2 => Some(Self::Changed),
            3 => Some(Self::SelfChecked),
            4 => Some(Self::GatesPassed),
            5 => Some(Self::ReviewPending),
            6 => Some(Self::Qualified),
            _ => None,
        }
    }
    #[doc = " Stable increasing qualification rank."]
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Observed => 1,
            Self::Changed => 2,
            Self::SelfChecked => 3,
            Self::GatesPassed => 4,
            Self::ReviewPending => 5,
            Self::Qualified => 6,
        }
    }
    #[doc = " Compares qualification strength; this does not authorize a state transition."]
    #[must_use]
    pub const fn permits(self, next: Self) -> bool {
        self.rank() <= next.rank()
    }
}
