//! Pure monotonic checkpoint and exactly-once settlement reducer.

use crate::{CandidateCheckpoint, RunSettlement, SettlementCause, SettlementError, SettlementErrorKind};
use vstd::prelude::*;

verus! {

/// In-memory pure reducer for one coding run.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SettlementReducer {
    checkpoint: Option<CandidateCheckpoint>,
    terminal: Option<RunSettlement>,
}

impl SettlementReducer {
    /// Logical view of the latest admitted candidate checkpoint.
    pub closed spec fn spec_checkpoint(&self) -> Option<CandidateCheckpoint> { self.checkpoint }
    /// Logical view of the sole retained terminal settlement.
    pub closed spec fn spec_terminal(&self) -> Option<RunSettlement> { self.terminal }

    /// Exact admission failure for an observation, with terminal rejection taking precedence.
    pub open spec fn spec_observe_error(
        &self,
        checkpoint: CandidateCheckpoint,
    ) -> Option<SettlementErrorKind> {
        if self.spec_terminal().is_some() {
            Some(SettlementErrorKind::AlreadySettled)
        } else {
            match self.spec_checkpoint() {
                Some(previous) => checkpoint.spec_successor_error(&previous),
                None => None,
            }
        }
    }

    /// Creates an empty, unsettled reducer.
    #[must_use]
    pub const fn new() -> (state: Self)
        ensures state.spec_checkpoint().is_none(), state.spec_terminal().is_none(),
    { Self { checkpoint: None, terminal: None } }

    /// Admits a strictly advancing candidate checkpoint.
    ///
    /// # Errors
    ///
    /// Rejects observations after settlement or an invalid successor checkpoint.
    pub fn observe(&mut self, checkpoint: CandidateCheckpoint) -> (result: Result<(), SettlementError>)
        ensures
            final(self).spec_terminal() == old(self).spec_terminal(),
            result.is_ok() == old(self).spec_observe_error(checkpoint).is_none(),
            match result {
                Ok(()) => final(self).spec_checkpoint() == Some(checkpoint),
                Err(error) => *final(self) == *old(self)
                    && Some(error.spec_kind()) == old(self).spec_observe_error(checkpoint),
            },
    {
        if !crate::verified::terminal_transition_allowed(self.terminal.is_some()) {
            return Err(SettlementError::new(SettlementErrorKind::AlreadySettled));
        }
        if let Some(previous) = &self.checkpoint {
            checkpoint.validate_successor(previous)?;
        }
        self.checkpoint = Some(checkpoint);
        Ok(())
    }

    /// Derives and records the sole terminal settlement.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementErrorKind::AlreadySettled`] after the first terminal decision.
    pub fn settle(&mut self, cause: SettlementCause) -> (result: Result<RunSettlement, SettlementError>)
        ensures
            final(self).spec_checkpoint() == old(self).spec_checkpoint(),
            result.is_ok() == old(self).spec_terminal().is_none(),
            match result {
                Ok(settlement) => final(self).spec_terminal() == Some(settlement)
                    && settlement.spec_cause() == cause
                    && settlement.spec_checkpoint() == old(self).spec_checkpoint()
                    && settlement.spec_disposition() == RunSettlement::spec_expected_disposition(
                        old(self).spec_checkpoint(), cause),
                Err(error) => *final(self) == *old(self)
                    && error.spec_kind() == SettlementErrorKind::AlreadySettled,
            },
    {
        if !crate::verified::terminal_transition_allowed(self.terminal.is_some()) {
            return Err(SettlementError::new(SettlementErrorKind::AlreadySettled));
        }
        let settlement = RunSettlement::decide(self.checkpoint.as_ref(), cause);
        self.terminal = Some(settlement);
        Ok(settlement)
    }

    /// Strongest admitted candidate checkpoint.
    #[must_use]
    pub const fn checkpoint(&self) -> (value: Option<&CandidateCheckpoint>)
        ensures match value {
            Some(checkpoint) => self.spec_checkpoint() == Some(*checkpoint),
            None => self.spec_checkpoint().is_none(),
        },
    { self.checkpoint.as_ref() }

    /// Terminal settlement after the run has settled.
    #[must_use]
    pub const fn terminal(&self) -> (value: Option<&RunSettlement>)
        ensures match value {
            Some(terminal) => self.spec_terminal() == Some(*terminal),
            None => self.spec_terminal().is_none(),
        },
    { self.terminal.as_ref() }
}

} // verus!
