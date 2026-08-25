//! Phase-local dispatch for already fenced E0 commands.

mod acceptance;
mod directives;
mod role_cycle;

use peritus_types::EventId;

use crate::state::mutation::{self, CounterKind};
use crate::{
    ChildObservation, OrchestratorCommandKind, OrchestratorError, OrchestratorEventKind,
    OrchestratorState, TerminalCause,
};

pub(super) fn record_observation(
    state: &mut OrchestratorState,
    observation: ChildObservation,
) -> Result<(), OrchestratorError> {
    mutation::increment_counter(state, CounterKind::RetainedObservations)?;
    mutation::push_observation(state, observation);
    Ok(())
}

pub(super) fn apply(
    state: &mut OrchestratorState,
    event_id: EventId,
    command: &OrchestratorCommandKind,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    match command {
        OrchestratorCommandKind::Start { .. } => {
            Err(crate::reducer::illegal("Start is legal only at genesis"))
        }
        OrchestratorCommandKind::PublishDirective { directive } => {
            directives::publish(state, event_id, directive)
        }
        OrchestratorCommandKind::AcknowledgeDirective { directive_id } => {
            directives::acknowledge(state, *directive_id)
        }
        OrchestratorCommandKind::ObserveHandoffActivation { activation } => {
            role_cycle::observe_activation(state, activation)
        }
        OrchestratorCommandKind::ObserveWriter { observation, candidate, quality_cycle } => {
            role_cycle::observe_writer(
                state,
                observation,
                candidate.as_ref(),
                quality_cycle.as_ref(),
            )
        }
        OrchestratorCommandKind::ObserveGates { observation, review_handoff } => {
            role_cycle::observe_gates(state, observation, review_handoff.as_ref())
        }
        OrchestratorCommandKind::ObserveReview { observation, fixer_handoff } => {
            role_cycle::observe_review(state, observation, fixer_handoff.as_ref())
        }
        OrchestratorCommandKind::ObserveFixer { completion } => {
            role_cycle::observe_fixer(state, completion)
        }
        OrchestratorCommandKind::ObserveRoleInfrastructure { scheduler, collaboration } => {
            role_cycle::observe_infrastructure(state, scheduler, collaboration)
        }
        OrchestratorCommandKind::AdvanceCandidate { quality_cycle } => {
            acceptance::advance_candidate(state, quality_cycle)
        }
        OrchestratorCommandKind::RecordAcceptanceCertificate { certificate } => {
            acceptance::record_certificate(state, certificate)
        }
        OrchestratorCommandKind::ObserveKernelAcceptance { observation } => {
            acceptance::observe_kernel(state, *observation)
        }
        OrchestratorCommandKind::Pause { reconciliation } => {
            acceptance::pause(state, reconciliation)
        }
        OrchestratorCommandKind::Resume { reconciliation } => {
            acceptance::resume(state, reconciliation)
        }
        OrchestratorCommandKind::Cancel { cause_digest } => {
            acceptance::cancel(state, *cause_digest)
        }
        OrchestratorCommandKind::ReconcileCancellation { observation } => {
            acceptance::reconcile(state, observation)
        }
        OrchestratorCommandKind::Reject { cause_digest } => {
            acceptance::explicit_terminal(state, TerminalCause::ExplicitRejection, *cause_digest)
                .map(|terminal| OrchestratorEventKind::Rejected { terminal })
        }
        OrchestratorCommandKind::Fail { cause_digest } => {
            acceptance::explicit_terminal(state, TerminalCause::ExplicitFailure, *cause_digest)
                .map(|terminal| OrchestratorEventKind::Failed { terminal })
        }
        OrchestratorCommandKind::Exhaust { cause_digest } => {
            acceptance::explicit_terminal(state, TerminalCause::ExplicitExhaustion, *cause_digest)
                .map(|terminal| OrchestratorEventKind::Exhausted { terminal })
        }
        OrchestratorCommandKind::Finalize => acceptance::finalize(state),
    }
}
