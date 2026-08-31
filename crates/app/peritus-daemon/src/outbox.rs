//! Durable C0 outbox delivery supervision.

use std::{future::Future, pin::Pin};

use crate::DaemonError;

mod claims;
mod clock;
mod handlers;
mod pump;
mod qualification;
mod router;

pub use claims::{CLAIM_DESTINATIONS, OrchestratorDirectiveClaim, TypedOutboxClaim, decode_claim};
pub use pump::OutboxRuntime;
pub use qualification::{
    recover_blob_after_crash, recover_blob_before_crash, recover_gate_after_crash,
    recover_gate_before_crash, recover_journal_before_crash, recover_lease_after_crash,
    recover_lease_before_crash, recover_outbox_crash, recover_patch_after_crash,
    recover_patch_before_crash, recover_promotion_after_crash, recover_promotion_before_crash,
    recover_snapshot_after_crash, recover_snapshot_before_crash, stage_blob_after_crash,
    stage_blob_before_crash, stage_gate_after_crash, stage_gate_before_crash,
    stage_journal_before_crash, stage_lease_after_crash, stage_lease_before_crash,
    stage_outbox_crash, stage_patch_after_crash, stage_patch_before_crash,
    stage_promotion_after_crash, stage_promotion_before_crash, stage_snapshot_after_crash,
    stage_snapshot_before_crash,
};
pub use router::DestinationRouter;

pub type DurableDelivery<'a> = Pin<Box<dyn Future<Output = Result<(), DaemonError>> + Send + 'a>>;

/// Exact claim not handled by the native authority child adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurableOutboxClaim {
    /// E0 D0-agent work.
    OrchestratorAgent(OrchestratorDirectiveClaim),
    /// E0 pure B2 evaluation.
    OrchestratorQualityEvaluator(OrchestratorDirectiveClaim),
    /// E0 B0 lifecycle transition.
    OrchestratorKernel(OrchestratorDirectiveClaim),
    /// E1 workspace materialization.
    HarnessMaterialization(claims::HarnessMaterializationClaim),
    /// E2 model analysis.
    DebuggerModel(peritus_debugger::ModelDirectiveClaim),
    /// E2 evidence publication.
    DebuggerPublication(peritus_debugger::PublicationDirectiveClaim),
    /// E3 D3 scheduling.
    EvaluationSchedule(peritus_eval::ScheduleDirectiveClaim),
    /// E3 rollout execution.
    EvaluationExecution(peritus_eval::ExecutionDirectiveClaim),
    /// E3 report publication.
    EvaluationPublication(peritus_eval::PublicationDirectiveClaim),
    /// F0 evolution evidence publication.
    EvolutionPublication(peritus_evolution::EvolutionPublicationClaim),
}

/// Authority-owned completion port for destinations whose native settlement needs resources that
/// cannot be borrowed by the outbox worker.
///
/// Implementations must perform the exact idempotent effect and settle the supplied native claim
/// through its owning subsystem API before returning `Ok(())`. In particular, a successful return
/// must mean that the owning API acknowledged C0's exact claim fence, either atomically with its C0
/// transition or after an idempotently replayable cross-store effect. A plain generic outbox
/// acknowledgement is not a valid implementation.
pub trait DurableOutboxPort: Send + Sync + 'static {
    /// Delivers and durably settles one already decoded E0, E1, E2, E3, or F0 claim.
    fn deliver_and_settle(&self, claim: DurableOutboxClaim) -> DurableDelivery<'_>;
}
