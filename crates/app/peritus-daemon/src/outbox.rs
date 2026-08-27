//! Durable C0 outbox delivery supervision.

use std::{future::Future, pin::Pin};

use crate::DaemonError;

mod claims;
mod clock;
mod handlers;
mod pump;
mod router;

pub(crate) use claims::{
    CLAIM_DESTINATIONS, OrchestratorDirectiveClaim, TypedOutboxClaim, decode_claim,
};
pub(crate) use pump::OutboxRuntime;
pub(crate) use router::DestinationRouter;

pub(crate) type DurableDelivery<'a> =
    Pin<Box<dyn Future<Output = Result<(), DaemonError>> + Send + 'a>>;

/// Exact claim not handled by the native authority child adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DurableOutboxClaim {
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
pub(crate) trait DurableOutboxPort: Send + Sync + 'static {
    /// Delivers and durably settles one already decoded E0, E1, E2, E3, or F0 claim.
    fn deliver_and_settle<'a>(&'a self, claim: DurableOutboxClaim) -> DurableDelivery<'a>;
}
