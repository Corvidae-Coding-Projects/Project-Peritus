//! One-transition driver preserving commit-before-effect ordering.

use peritus_journal::{CommittedBatch, SqliteJournal};

use crate::{OrchestratorCommand, OrchestratorEvent, OrchestratorState};

use super::{DirectivePublisher, DirectiveReceipt};

/// Result of one durably committed E0 transition and optional post-commit publication.
#[derive(Debug, Eq, PartialEq)]
pub struct DriverStep {
    event: OrchestratorEvent,
    batch: CommittedBatch,
    receipt: Option<DirectiveReceipt>,
}

impl DriverStep {
    /// Returns the durably committed E0 event.
    #[must_use]
    pub const fn event(&self) -> &OrchestratorEvent {
        &self.event
    }
    /// Returns the atomic C0 commit result.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }
    /// Returns the post-commit directive receipt when publication occurred.
    #[must_use]
    pub const fn receipt(&self) -> Option<DirectiveReceipt> {
        self.receipt
    }
}

/// In-memory cursor whose state advances only after C0 accepts the exact transition.
#[derive(Debug, Default)]
pub struct OrchestratorDriver {
    state: Option<OrchestratorState>,
}

impl OrchestratorDriver {
    /// Creates a driver for a new aggregate.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: None }
    }

    /// Creates a driver at an exactly replayed durable checkpoint.
    #[must_use]
    pub const fn recovered(state: OrchestratorState) -> Self {
        Self { state: Some(state) }
    }

    /// Recovers a driver by replaying C0 and matching its checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when C0 loading, replay, or checkpoint verification fails.
    pub fn recover(
        journal: &SqliteJournal,
        run_id: peritus_types::RunId,
    ) -> Result<Self, crate::OrchestratorError> {
        let replay = crate::durability::load_orchestrator_replay(journal, run_id)?;
        Ok(Self { state: replay.rebuild()? })
    }

    #[must_use]
    /// Returns the currently installed durable successor state.
    pub const fn state(&self) -> Option<&OrchestratorState> {
        self.state.as_ref()
    }

    /// Reduces, canonically commits, installs the successor, then performs at most one effect.
    ///
    /// # Errors
    ///
    /// Returns an error when reduction, C0 commit, publication, or receipt verification fails.
    pub fn step(
        &mut self,
        journal: &mut SqliteJournal,
        publisher: &mut impl DirectivePublisher,
        command: &OrchestratorCommand,
    ) -> Result<DriverStep, crate::OrchestratorError> {
        let transition = match &self.state {
            Some(state) => crate::reducer::decide(state, command)?,
            None => crate::reducer::start(command)?,
        };
        let batch =
            crate::durability::commit_orchestrator_transition(journal, command, &transition)?;
        let event = transition.event().clone();
        self.state = Some(transition.into_state());
        let receipt =
            if matches!(event.kind(), crate::OrchestratorEventKind::DirectivePublished { .. }) {
                let directive =
                    self.state.as_ref().and_then(OrchestratorState::pending_directive).ok_or_else(
                        || integrity("committed directive event lacks successor directive"),
                    )?;
                let receipt = publisher.publish(directive)?;
                if !receipt.matches(directive) {
                    return Err(integrity("publisher receipt differs from committed directive"));
                }
                Some(receipt)
            } else {
                None
            };
        Ok(DriverStep { event, batch, receipt })
    }
}

const fn integrity(detail: &'static str) -> crate::OrchestratorError {
    crate::OrchestratorError::new(
        crate::OrchestratorErrorKind::Integrity,
        crate::OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
