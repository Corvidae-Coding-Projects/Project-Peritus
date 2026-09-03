//! Stable checked-construction and reducer failures.

use vstd::prelude::*;

verus! {

/// Stable reason a candidate checkpoint or settlement transition was rejected.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettlementErrorKind {
    /// Checkpoint sequence zero is reserved and cannot identify an observation.
    ZeroCheckpointSequence,
    /// A checkpoint named a different run or workspace than its predecessor.
    CandidateLineageMismatch,
    /// A checkpoint sequence did not strictly advance.
    CheckpointDidNotAdvance,
    /// The same candidate regressed to an earlier qualification stage.
    CandidateStageRegressed,
    /// Evidence marked current or failed did not bind the current candidate.
    CurrentEvidenceBindingMismatch,
    /// Evidence marked stale still binds the current candidate.
    StaleEvidenceBindingMismatch,
    /// The declared candidate stage is not supported by its evidence.
    CandidateStageEvidenceMismatch,
    /// The reducer was asked to observe or settle after reaching a terminal state.
    AlreadySettled,
}

/// Typed failure with no effectful or free-form diagnostic payload.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SettlementError {
    kind: SettlementErrorKind,
}

impl SettlementError {
    pub(crate) const fn new(kind: SettlementErrorKind) -> Self { Self { kind } }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> SettlementErrorKind { self.kind }
}

} // verus!

#[cfg(not(verus_only))]
impl core::fmt::Display for SettlementError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "run settlement rejected: {:?}", self.kind())
    }
}

#[cfg(not(verus_only))]
impl std::error::Error for SettlementError {}
