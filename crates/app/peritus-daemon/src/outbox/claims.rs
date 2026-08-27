//! Typed, effect-free decoding of claimed C0 outbox rows.

mod domains;
mod orchestrator;

use peritus_journal::{OutboxMessage, OutboxState};
use peritus_orchestrator::DirectiveDestination;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(crate) use domains::{
    HarnessMaterializationClaim, decode_debugger_model_claim, decode_debugger_publication_claim,
    decode_evaluation_execution_claim, decode_evaluation_publication_claim,
    decode_evaluation_schedule_claim, decode_evolution_publication_claim,
    decode_harness_materialization_claim,
};
pub(crate) use orchestrator::{OrchestratorDirectiveClaim, decode_orchestrator_claim};

/// Exact harness materialization destination, which is private to the harness durability module.
pub(crate) const HARNESS_MATERIALIZATION_DESTINATION: &str = "peritus.harness.materialize.v1";

/// Complete closed destination inventory admitted by the production claim decoder.
pub(crate) const CLAIM_DESTINATIONS: [&str; 14] = [
    DirectiveDestination::Scheduler.outbox_destination(),
    DirectiveDestination::Collaboration.outbox_destination(),
    DirectiveDestination::Agent.outbox_destination(),
    DirectiveDestination::Gates.outbox_destination(),
    DirectiveDestination::Review.outbox_destination(),
    DirectiveDestination::QualityEvaluator.outbox_destination(),
    DirectiveDestination::Kernel.outbox_destination(),
    HARNESS_MATERIALIZATION_DESTINATION,
    peritus_debugger::MODEL_ANALYSIS_DESTINATION,
    peritus_debugger::PUBLICATION_DESTINATION,
    peritus_eval::SCHEDULE_DESTINATION,
    peritus_eval::EXECUTION_DESTINATION,
    peritus_eval::PUBLICATION_DESTINATION,
    peritus_evolution::EVOLUTION_PUBLICATION_DESTINATION,
];

/// One fully decoded, inert claim from the closed production destination inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypedOutboxClaim {
    /// E0 scheduler directive.
    OrchestratorScheduler(OrchestratorDirectiveClaim),
    /// E0 collaboration directive.
    OrchestratorCollaboration(OrchestratorDirectiveClaim),
    /// E0 agent directive.
    OrchestratorAgent(OrchestratorDirectiveClaim),
    /// E0 gates directive.
    OrchestratorGates(OrchestratorDirectiveClaim),
    /// E0 independent review directive.
    OrchestratorReview(OrchestratorDirectiveClaim),
    /// E0 pure quality-evaluator directive.
    OrchestratorQualityEvaluator(OrchestratorDirectiveClaim),
    /// E0 lifecycle-kernel directive.
    OrchestratorKernel(OrchestratorDirectiveClaim),
    /// E1 exact materialization plan.
    HarnessMaterialization(HarnessMaterializationClaim),
    /// E2 provider-neutral model-analysis directive.
    DebuggerModel(peritus_debugger::ModelDirectiveClaim),
    /// E2 report publication directive.
    DebuggerPublication(peritus_debugger::PublicationDirectiveClaim),
    /// E3 D3 scheduling directive.
    EvaluationSchedule(peritus_eval::ScheduleDirectiveClaim),
    /// E3 candidate/evaluator execution directive.
    EvaluationExecution(peritus_eval::ExecutionDirectiveClaim),
    /// E3 report publication directive.
    EvaluationPublication(peritus_eval::PublicationDirectiveClaim),
    /// F0 evidence publication directive.
    EvolutionPublication(peritus_evolution::EvolutionPublicationClaim),
}

/// Decodes one claimed row using the exact decoder selected by its closed destination.
///
/// # Errors
/// Returns a corrupt-state error for a malformed or inconsistent known claim and an unsupported
/// error for a destination outside the production inventory.
pub(crate) fn decode_claim(message: &OutboxMessage) -> Result<TypedOutboxClaim, DaemonError> {
    let destination = message.destination();
    match destination {
        value if value == DirectiveDestination::Scheduler.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::Scheduler)
                .map(TypedOutboxClaim::OrchestratorScheduler)
        }
        value if value == DirectiveDestination::Collaboration.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::Collaboration)
                .map(TypedOutboxClaim::OrchestratorCollaboration)
        }
        value if value == DirectiveDestination::Agent.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::Agent)
                .map(TypedOutboxClaim::OrchestratorAgent)
        }
        value if value == DirectiveDestination::Gates.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::Gates)
                .map(TypedOutboxClaim::OrchestratorGates)
        }
        value if value == DirectiveDestination::Review.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::Review)
                .map(TypedOutboxClaim::OrchestratorReview)
        }
        value if value == DirectiveDestination::QualityEvaluator.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::QualityEvaluator)
                .map(TypedOutboxClaim::OrchestratorQualityEvaluator)
        }
        value if value == DirectiveDestination::Kernel.outbox_destination() => {
            decode_orchestrator_claim(message, DirectiveDestination::Kernel)
                .map(TypedOutboxClaim::OrchestratorKernel)
        }
        HARNESS_MATERIALIZATION_DESTINATION => decode_harness_materialization_claim(message)
            .map(TypedOutboxClaim::HarnessMaterialization),
        peritus_debugger::MODEL_ANALYSIS_DESTINATION => {
            decode_debugger_model_claim(message).map(TypedOutboxClaim::DebuggerModel)
        }
        peritus_debugger::PUBLICATION_DESTINATION => {
            decode_debugger_publication_claim(message).map(TypedOutboxClaim::DebuggerPublication)
        }
        peritus_eval::SCHEDULE_DESTINATION => {
            decode_evaluation_schedule_claim(message).map(TypedOutboxClaim::EvaluationSchedule)
        }
        peritus_eval::EXECUTION_DESTINATION => {
            decode_evaluation_execution_claim(message).map(TypedOutboxClaim::EvaluationExecution)
        }
        peritus_eval::PUBLICATION_DESTINATION => decode_evaluation_publication_claim(message)
            .map(TypedOutboxClaim::EvaluationPublication),
        peritus_evolution::EVOLUTION_PUBLICATION_DESTINATION => {
            decode_evolution_publication_claim(message).map(TypedOutboxClaim::EvolutionPublication)
        }
        _ => Err(DaemonError::new(
            DaemonErrorCode::Unsupported,
            DaemonRecovery::Operator,
            "decode outbox claim",
            "outbox destination is outside the closed production inventory",
        )),
    }
}

pub(super) fn require_claimed(
    message: &OutboxMessage,
    expected_destination: &str,
) -> Result<u64, DaemonError> {
    if message.state() != OutboxState::Claimed {
        return Err(invalid_claim("outbox row is not in claimed state"));
    }
    if message.destination() != expected_destination {
        return Err(invalid_claim("outbox destination differs from the selected typed decoder"));
    }
    message
        .fence()
        .filter(|fence| *fence > 0)
        .ok_or_else(|| invalid_claim("claimed outbox row has no positive fence"))
}

pub(super) fn invalid_claim(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "decode outbox claim",
        detail,
    )
}

pub(super) fn domain_claim_error(
    detail: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "decode outbox claim",
        detail,
        source,
    )
}
