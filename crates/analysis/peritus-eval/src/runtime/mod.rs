//! Narrow effect orchestration over existing C0 owners.

mod artifact;
mod publication;
mod recovery;

use peritus_journal::{CommittedBatch, SqliteJournal};
use peritus_types::{CommandId, EventId};

pub use artifact::{
    FinalizedEvaluationArtifact, commit_report_ready, finalize_report_artifact,
    stage_and_commit_report,
};
pub use publication::{PublicationExecution, publish_claimed_report};
pub use recovery::{EvaluationRecoveryDecision, RecoveryObservation, decide_recovery};

use crate::{
    EvaluationCommand, EvaluationError, EvaluationState, EvaluationTransition,
    commit_evaluation_transition,
};

/// Caller-reserved command/event identities for one exact transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionIds {
    command_id: CommandId,
    event_id: EventId,
}

impl TransitionIds {
    /// Binds caller-reserved C0 identities.
    #[must_use]
    pub const fn new(command_id: CommandId, event_id: EventId) -> Self {
        Self { command_id, event_id }
    }
    /// Command identity.
    #[must_use]
    pub const fn command_id(self) -> CommandId {
        self.command_id
    }
    /// Event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
}

/// One committed C0 batch paired with its exact successor state.
#[derive(Debug)]
pub struct CommittedEvaluationTransition {
    batch: CommittedBatch,
    state: EvaluationState,
}

impl CommittedEvaluationTransition {
    pub(crate) const fn new(batch: CommittedBatch, state: EvaluationState) -> Self {
        Self { batch, state }
    }
    /// Opaque C0 commit observation.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }
    /// Exact successor state.
    #[must_use]
    pub const fn state(&self) -> &EvaluationState {
        &self.state
    }
    /// Consumes the complete result.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, EvaluationState) {
        (self.batch, self.state)
    }
}

/// Production E3 runtime facade over the single C0 journal owner.
pub struct EvaluationRuntime<'a> {
    journal: &'a mut SqliteJournal,
}

impl<'a> EvaluationRuntime<'a> {
    /// Borrows the externally owned journal mutably for this runtime turn.
    #[must_use]
    pub const fn new(journal: &'a mut SqliteJournal) -> Self {
        Self { journal }
    }

    /// Commits an already pure-decided ordinary transition.
    ///
    /// # Errors
    /// Returns the stable E3 failure when C0 rejects or cannot commit the transition.
    pub fn commit(
        &mut self,
        command: &EvaluationCommand,
        transition: &EvaluationTransition,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        let batch = commit_evaluation_transition(self.journal, command, transition)?;
        Ok(CommittedEvaluationTransition::new(batch, transition.state().clone()))
    }

    /// Borrows the underlying journal for C0 claim/replay composition.
    #[must_use]
    pub const fn journal(&mut self) -> &mut SqliteJournal {
        self.journal
    }
}
