//! Immutable terminal settlement derived from the strongest candidate checkpoint.

use crate::{CandidateCheckpoint, RunDisposition, SettlementCause};
use vstd::prelude::*;

verus! {

/// Exact terminal truth for one run and its optional strongest candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RunSettlement {
    disposition: RunDisposition,
    cause: SettlementCause,
    checkpoint: Option<CandidateCheckpoint>,
}

impl RunSettlement {
    #[allow(
        clippy::manual_map,
        clippy::option_if_let_else,
        reason = "the equivalent Option combinators are not supported by the Verus frontend"
    )]
    pub(crate) fn decide(
        checkpoint: Option<&CandidateCheckpoint>,
        cause: SettlementCause,
    ) -> Self {
        let qualified = match checkpoint {
            Some(candidate) => candidate.is_qualified(),
            None => false,
        };
        let disposition = match cause {
            SettlementCause::UserWait => RunDisposition::WaitingForUser,
            SettlementCause::Cancellation => RunDisposition::Cancelled,
            SettlementCause::Recovery => RunDisposition::RecoveryRequired,
            _ if qualified => RunDisposition::Accepted,
            _ if checkpoint.is_some() => RunDisposition::CandidateAvailable,
            _ => RunDisposition::FailedNoCandidate,
        };
        let checkpoint = match checkpoint {
            Some(candidate) => Some(*candidate),
            None => None,
        };
        Self { disposition, cause, checkpoint }
    }

    /// Honest user-visible terminal disposition.
    #[must_use]
    pub const fn disposition(&self) -> RunDisposition { self.disposition }

    /// Typed cause that ended active execution.
    #[must_use]
    pub const fn cause(&self) -> SettlementCause { self.cause }

    /// Strongest exact candidate observed before settlement.
    #[must_use]
    pub const fn checkpoint(&self) -> Option<&CandidateCheckpoint> {
        self.checkpoint.as_ref()
    }

    /// Whether strict automated qualification accepted the candidate.
    #[must_use]
    pub const fn is_accepted(&self) -> bool { self.disposition.is_accepted() }
}

} // verus!
