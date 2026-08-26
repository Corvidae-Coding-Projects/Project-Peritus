//! Narrow effect orchestration over existing C0/C5/C6 owners.

mod artifact;
mod model;
mod publication;
mod recovery;

use peritus_journal::CommittedBatch;
use peritus_types::{CommandId, EventId};

pub use artifact::{
    FinalizedReportArtifact, commit_report_ready, finalize_report_artifact, report_record,
    stage_and_commit_report,
};
pub use model::{
    ModelAttemptExecution, ModelAttemptIds, ModelAttemptOutcome, execute_model_attempt,
    schedule_model_retry,
};
pub use publication::{PublicationExecution, publish_claimed_report};
pub use recovery::{DebuggerRecoveryDecision, decide_recovery};

use crate::DebuggerState;

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

/// One committed batch paired with its exact successor state.
#[derive(Debug)]
pub struct CommittedDebuggerTransition {
    batch: CommittedBatch,
    state: DebuggerState,
}

impl CommittedDebuggerTransition {
    pub(crate) const fn new(batch: CommittedBatch, state: DebuggerState) -> Self {
        Self { batch, state }
    }
    /// Opaque C0 commit observation.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }
    /// Exact successor state.
    #[must_use]
    pub const fn state(&self) -> &DebuggerState {
        &self.state
    }
    /// Consumes the result.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, DebuggerState) {
        (self.batch, self.state)
    }
}
