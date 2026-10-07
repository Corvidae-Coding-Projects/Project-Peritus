//! Narrow effect orchestration over existing C0/C5/C6 owners.

mod artifact;
mod model;
mod publication;
mod recovery;

use peritus_journal::{CommittedBatch, OutboxId};
use peritus_types::{CommandId, EventId};

pub use artifact::{
    FinalizedReportArtifact, commit_report_ready, finalize_report_artifact, report_record,
    stage_and_commit_report,
};
pub(super) use artifact::verify_report_artifact;
pub use model::{
    ModelAttemptExecution, ModelAttemptIds, ModelAttemptOutcome, ResumedModelAttemptExecution,
    amend_model_retry_policy, execute_model_attempt, resume_model_attempt, schedule_model_retry,
};
pub use publication::{
    CompletedPublicationRepair, PublicationExecution, observe_publication_dependencies,
    publish_claimed_report, reconcile_interrupted_publication, repair_completed_publication,
};
pub use recovery::{
    DebuggerRecoveryDecision, PublicationDeliveryObservation, PublicationDependencyRepair,
    PublicationDependencyStatus, PublicationRecoveryObservation, decide_publication_recovery,
    decide_recovery, decide_recovery_on_clock, decide_recovery_on_clock_with_delivery,
    decide_recovery_with_delivery,
};

use crate::{
    CommittedDebuggerOperation, DebuggerCommitMode, DebuggerError, DebuggerErrorKind,
    DebuggerEvent, DebuggerOperation, DebuggerOperationReceipt, DebuggerRecovery, DebuggerState,
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

/// One committed batch paired with its exact successor state.
#[derive(Debug)]
pub struct CommittedDebuggerTransition {
    operation: CommittedDebuggerOperation,
}

impl CommittedDebuggerTransition {
    pub(crate) const fn new(operation: CommittedDebuggerOperation) -> Self {
        Self { operation }
    }
    /// Opaque C0 commit observation.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        self.operation.batch()
    }
    /// Current state reconstructed independently after observing the accepted operation.
    #[must_use]
    pub const fn state(&self) -> &DebuggerState {
        self.operation.current_state()
    }
    /// Exact historical successor produced by this operation.
    #[must_use]
    pub const fn historical_state(&self) -> &DebuggerState {
        self.operation.historical_state()
    }
    /// Original request and claim receipt accepted for this operation.
    #[must_use]
    pub const fn receipt(&self) -> DebuggerOperationReceipt {
        self.operation.receipt()
    }
    /// Exact immutable event accepted for this operation.
    #[must_use]
    pub const fn event(&self) -> &DebuggerEvent {
        self.operation.event()
    }
    /// Whether this operation still produces the current debugger checkpoint.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.operation.is_current()
    }
    /// Consumes the result into its original batch and current state.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, DebuggerState) {
        let (batch, _, _, _, current) = self.operation.into_parts();
        (batch, current)
    }
    /// Consumes the result without discarding its historical outcome or original receipt.
    #[must_use]
    pub fn into_operation(self) -> CommittedDebuggerOperation {
        self.operation
    }
}

pub(super) fn validate_recovered_claim(
    receipt: DebuggerOperationReceipt,
    required_mode: DebuggerCommitMode,
    outbox_id: OutboxId,
    fence: u64,
    operation: DebuggerOperation,
) -> Result<(), DebuggerError> {
    if receipt.mode() == DebuggerCommitMode::Legacy {
        return Ok(());
    }
    let Some(original) = receipt.original_claim() else {
        return Err(recovery_error(
            operation,
            "recovered debugger effect has no retained original claim",
        ));
    };
    if receipt.mode() != required_mode
        || original.outbox_id() != outbox_id
        || fence < original.fence()
    {
        return Err(recovery_error(
            operation,
            "recovered debugger effect differs from the supplied claim authority",
        ));
    }
    Ok(())
}

fn recovery_error(operation: DebuggerOperation, detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Recovery,
        operation,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
