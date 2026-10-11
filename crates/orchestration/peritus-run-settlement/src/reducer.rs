//! Pure monotonic checkpoint and exactly-once settlement reducer.

use crate::{
    CandidateCheckpoint, RunSettlement, SettlementCause, SettlementError, SettlementErrorKind,
};

#[doc = " In-memory pure reducer for one coding run."]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SettlementReducer {
    checkpoint: Option<CandidateCheckpoint>,
    terminal: Option<RunSettlement>,
}
impl SettlementReducer {
    #[doc = " Creates an empty, unsettled reducer."]
    #[must_use]
    pub const fn new() -> Self {
        Self { checkpoint: None, terminal: None }
    }
    #[doc = " Admits a strictly advancing candidate checkpoint."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects observations after settlement or an invalid successor checkpoint."]
    pub fn observe(&mut self, checkpoint: CandidateCheckpoint) -> Result<(), SettlementError> {
        if !crate::verified::terminal_transition_allowed(self.terminal.is_some()) {
            return Err(SettlementError::new(SettlementErrorKind::AlreadySettled));
        }
        if let Some(previous) = &self.checkpoint {
            checkpoint.validate_successor(previous)?;
        }
        self.checkpoint = Some(checkpoint);
        Ok(())
    }
    #[doc = " Derives and records the sole terminal settlement."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`SettlementErrorKind::AlreadySettled`] after the first terminal decision."]
    pub fn settle(&mut self, cause: SettlementCause) -> Result<RunSettlement, SettlementError> {
        if !crate::verified::terminal_transition_allowed(self.terminal.is_some()) {
            return Err(SettlementError::new(SettlementErrorKind::AlreadySettled));
        }
        let settlement = RunSettlement::decide(self.checkpoint.as_ref(), cause);
        self.terminal = Some(settlement);
        Ok(settlement)
    }
    #[doc = " Latest admitted candidate checkpoint."]
    #[must_use]
    pub const fn checkpoint(&self) -> Option<&CandidateCheckpoint> {
        self.checkpoint.as_ref()
    }
    #[doc = " Terminal settlement after the run has settled."]
    #[must_use]
    pub const fn terminal(&self) -> Option<&RunSettlement> {
        self.terminal.as_ref()
    }
}
