//! Closed execution-class vocabulary used by work and worker selection.

#[doc = " Closed supported execution class."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionClass {
    #[doc = " Model inference or agent turn."]
    Model,
    #[doc = " Inert tool request handled by a governed router."]
    Tool,
    #[doc = " Acceptance-gate execution."]
    Gate,
    #[doc = " Independent review work."]
    Review,
    #[doc = " Orchestration/control-plane work."]
    Coordination,
}
impl ExecutionClass {
    #[doc = " Compares canonical execution-class ranks for checked worker admission."]
    pub(crate) const fn precedes(self, other: Self) -> bool {
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
    #[doc = " Compares exact execution classes for verified selector lookup."]
    pub(crate) const fn same(self, other: Self) -> bool {
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
