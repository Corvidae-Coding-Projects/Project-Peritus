//! Native E1, E2, E3, and F0 claim bindings.

use peritus_journal::OutboxMessage;

use crate::DaemonError;

use super::{HARNESS_MATERIALIZATION_DESTINATION, domain_claim_error, require_claimed};

/// Exact decoded E1 plan and the domain-owned C0 claim required for settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HarnessMaterializationClaim {
    plan: peritus_harness::MaterializationPlan,
    claim: peritus_harness::DirectiveClaim,
}

impl HarnessMaterializationClaim {
    /// Borrows the exact canonical materialization plan.
    pub(crate) const fn plan(&self) -> &peritus_harness::MaterializationPlan {
        &self.plan
    }

    /// Returns the native claim accepted by harness settlement APIs.
    pub(crate) const fn claim(&self) -> peritus_harness::DirectiveClaim {
        self.claim
    }
}

/// Decodes an exact claimed E1 materialization plan through its native claim binding.
pub(crate) fn decode_harness_materialization_claim(
    message: &OutboxMessage,
) -> Result<HarnessMaterializationClaim, DaemonError> {
    require_claimed(message, HARNESS_MATERIALIZATION_DESTINATION)?;
    let plan = peritus_harness::MaterializationPlan::decode_canonical(message.payload()).map_err(
        |error| domain_claim_error("harness materialization payload is not canonical", error),
    )?;
    let claim = peritus_harness::DirectiveClaim::from_message(&plan, message).map_err(|error| {
        domain_claim_error("harness materialization claim binding is invalid", error)
    })?;
    Ok(HarnessMaterializationClaim { plan, claim })
}

/// Decodes an exact claimed E2 model-analysis directive through its native claim API.
pub(crate) fn decode_debugger_model_claim(
    message: &OutboxMessage,
) -> Result<peritus_debugger::ModelDirectiveClaim, DaemonError> {
    require_claimed(message, peritus_debugger::MODEL_ANALYSIS_DESTINATION)?;
    peritus_debugger::ModelDirectiveClaim::from_message(message).map_err(|error| {
        domain_claim_error("debugger model-analysis claim binding is invalid", error)
    })
}

/// Decodes an exact claimed E2 publication directive through its native claim API.
pub(crate) fn decode_debugger_publication_claim(
    message: &OutboxMessage,
) -> Result<peritus_debugger::PublicationDirectiveClaim, DaemonError> {
    require_claimed(message, peritus_debugger::PUBLICATION_DESTINATION)?;
    peritus_debugger::PublicationDirectiveClaim::from_message(message)
        .map_err(|error| domain_claim_error("debugger publication claim binding is invalid", error))
}

/// Decodes an exact claimed E3 scheduling directive through its native claim API.
pub(crate) fn decode_evaluation_schedule_claim(
    message: &OutboxMessage,
) -> Result<peritus_eval::ScheduleDirectiveClaim, DaemonError> {
    require_claimed(message, peritus_eval::SCHEDULE_DESTINATION)?;
    peritus_eval::ScheduleDirectiveClaim::from_message(message).map_err(|error| {
        domain_claim_error("evaluation scheduling claim binding is invalid", error)
    })
}

/// Decodes an exact claimed E3 execution directive through its native claim API.
pub(crate) fn decode_evaluation_execution_claim(
    message: &OutboxMessage,
) -> Result<peritus_eval::ExecutionDirectiveClaim, DaemonError> {
    require_claimed(message, peritus_eval::EXECUTION_DESTINATION)?;
    peritus_eval::ExecutionDirectiveClaim::from_message(message)
        .map_err(|error| domain_claim_error("evaluation execution claim binding is invalid", error))
}

/// Decodes an exact claimed E3 publication directive through its native claim API.
pub(crate) fn decode_evaluation_publication_claim(
    message: &OutboxMessage,
) -> Result<peritus_eval::PublicationDirectiveClaim, DaemonError> {
    require_claimed(message, peritus_eval::PUBLICATION_DESTINATION)?;
    peritus_eval::PublicationDirectiveClaim::from_message(message).map_err(|error| {
        domain_claim_error("evaluation publication claim binding is invalid", error)
    })
}

/// Decodes an exact claimed F0 publication directive through its native claim API.
pub(crate) fn decode_evolution_publication_claim(
    message: &OutboxMessage,
) -> Result<peritus_evolution::EvolutionPublicationClaim, DaemonError> {
    require_claimed(message, peritus_evolution::EVOLUTION_PUBLICATION_DESTINATION)?;
    peritus_evolution::EvolutionPublicationClaim::from_message(message).map_err(|error| {
        domain_claim_error("evolution publication claim binding is invalid", error)
    })
}
