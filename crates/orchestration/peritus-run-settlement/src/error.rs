//! Stable checked-construction and reducer failures.

#[doc = " Stable reason a candidate checkpoint or settlement transition was rejected."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettlementErrorKind {
    #[doc = " Checkpoint sequence zero is reserved and cannot identify an observation."]
    ZeroCheckpointSequence,
    #[doc = " A checkpoint named a different run or workspace than its predecessor."]
    CandidateLineageMismatch,
    #[doc = " A checkpoint sequence did not strictly advance."]
    CheckpointDidNotAdvance,
    #[doc = " Evidence marked current or failed did not bind the current candidate."]
    CurrentEvidenceBindingMismatch,
    #[doc = " Evidence marked stale still binds the current candidate."]
    StaleEvidenceBindingMismatch,
    #[doc = " The declared candidate stage is not supported by its evidence."]
    CandidateStageEvidenceMismatch,
    #[doc = " The reducer was asked to observe or settle after reaching a terminal state."]
    AlreadySettled,
}
#[doc = " Typed failure with no effectful or free-form diagnostic payload."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SettlementError {
    kind: SettlementErrorKind,
}
impl SettlementError {
    pub(crate) const fn new(kind: SettlementErrorKind) -> Self {
        Self { kind }
    }
    #[doc = " Returns the stable failure category."]
    #[must_use]
    pub const fn kind(&self) -> SettlementErrorKind {
        self.kind
    }
}

impl core::fmt::Display for SettlementError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "run settlement rejected: {:?}", self.kind())
    }
}

impl std::error::Error for SettlementError {}
