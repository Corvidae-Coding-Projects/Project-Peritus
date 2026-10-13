//! Typed reason a run reached settlement.

#[doc = " Stable terminal cause independent of candidate quality."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettlementCause {
    #[doc = " All intended phases reached their normal terminal boundary."]
    Completed,
    #[doc = " An explicitly authorized investigation completed with review and no patch."]
    NoChangeComplete,
    #[doc = " A material user answer is required."]
    UserWait,
    #[doc = " The user explicitly cancelled the run."]
    Cancellation,
    #[doc = " The caller\'s run horizon was exhausted."]
    Deadline,
    #[doc = " The selected provider reached a terminal condition."]
    Provider,
    #[doc = " The request remained outside the supported context envelope."]
    Context,
    #[doc = " Deterministic gates could not complete or pass."]
    Gate,
    #[doc = " Independent review could not complete or retained blockers."]
    Review,
    #[doc = " The managed repository prevented further work."]
    Repository,
    #[doc = " An adapter or report-projection boundary failed."]
    Adapter,
    #[doc = " A restart requires effectful reconciliation before work resumes."]
    Recovery,
    #[doc = " An internal invariant prevented a trustworthy continuation."]
    InternalInvariant,
}
impl SettlementCause {
    #[doc = " Stable protocol tag."]
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Completed => 1,
            Self::NoChangeComplete => 13,
            Self::UserWait => 2,
            Self::Cancellation => 3,
            Self::Deadline => 4,
            Self::Provider => 5,
            Self::Context => 6,
            Self::Gate => 7,
            Self::Review => 8,
            Self::Repository => 9,
            Self::Adapter => 10,
            Self::Recovery => 11,
            Self::InternalInvariant => 12,
        }
    }
    #[doc = " Decodes a stable protocol tag."]
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Completed),
            13 => Some(Self::NoChangeComplete),
            2 => Some(Self::UserWait),
            3 => Some(Self::Cancellation),
            4 => Some(Self::Deadline),
            5 => Some(Self::Provider),
            6 => Some(Self::Context),
            7 => Some(Self::Gate),
            8 => Some(Self::Review),
            9 => Some(Self::Repository),
            10 => Some(Self::Adapter),
            11 => Some(Self::Recovery),
            12 => Some(Self::InternalInvariant),
            _ => None,
        }
    }
}
