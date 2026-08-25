//! Cross-record semantic checks before an E0 C0 append.

use crate::{
    OrchestratorCommand, OrchestratorCommandKind, OrchestratorEventKind, OrchestratorTransition,
};

pub(super) fn validate_binding(
    command: &OrchestratorCommand,
    transition: &OrchestratorTransition,
) -> Result<(), crate::OrchestratorError> {
    let event = transition.event();
    let state = transition.state();
    let identity_matches = [
        command.event_id() == event.id(),
        command.command_id() == event.command_id(),
        command.run_id() == event.run_id(),
    ]
    .into_iter()
    .all(core::convert::identity);
    let predecessor_matches = [
        command.expected_previous_event() == event.previous_event(),
        command.expected_sequence().checked_add(1) == Some(event.sequence().get()),
        command.revision() == event.revision(),
        command.prior_state_digest() == event.prior_state_digest(),
    ]
    .into_iter()
    .all(core::convert::identity);
    let checkpoint_matches = [
        command.run_id() == state.binding().run_id(),
        event.successor_state_digest() == state.state_digest(),
        event.sequence() == state.sequence(),
        event.id() == state.last_event_id(),
    ]
    .into_iter()
    .all(core::convert::identity);
    let mismatch = ![
        identity_matches,
        predecessor_matches,
        checkpoint_matches,
        event_matches(command.kind(), event.kind()),
    ]
    .into_iter()
    .all(core::convert::identity);
    if mismatch {
        Err(super::integrity("E0 command, event, and checkpoint differ"))
    } else {
        Ok(())
    }
}

fn event_matches(command: &OrchestratorCommandKind, event: &OrchestratorEventKind) -> bool {
    use OrchestratorCommandKind as C;
    use OrchestratorEventKind as E;
    match (command, event) {
        (C::Start { genesis: command }, E::Started { genesis: event }) => command == event,
        (C::PublishDirective { directive: a }, E::DirectivePublished { directive: b }) => a == b,
        (
            C::AcknowledgeDirective { directive_id: a },
            E::DirectiveAcknowledged { directive_id: b },
        ) => a == b,
        (C::ObserveHandoffActivation { activation: a }, E::HandoffActivated { activation: b }) => {
            a == b
        }
        (
            C::ObserveWriter {
                observation: command_observation,
                candidate: command_candidate,
                quality_cycle: command_cycle,
            },
            E::WriterObserved {
                observation: event_observation,
                candidate: event_candidate,
                quality_cycle: event_cycle,
            },
        ) => {
            command_observation == event_observation
                && command_candidate == event_candidate
                && command_cycle == event_cycle
        }
        (
            C::ObserveGates { observation: a, review_handoff: b },
            E::GatesObserved { observation: x, review_handoff: y },
        ) => a == x && b == y,
        (
            C::ObserveReview { observation: a, fixer_handoff: b },
            E::ReviewObserved { observation: x, fixer_handoff: y },
        ) => a == x && b == y,
        (C::ObserveFixer { completion: a }, E::FixerObserved { completion: b }) => a == b,
        (
            C::ObserveRoleInfrastructure { scheduler: a, collaboration: b },
            E::RoleInfrastructureObserved { scheduler: x, collaboration: y },
        ) => a == x && b == y,
        (
            C::AdvanceCandidate { quality_cycle: a },
            E::CandidateAdvanced { quality_cycle: b, .. },
        ) => a == b,
        (
            C::RecordAcceptanceCertificate { certificate: a },
            E::AcceptanceCertificateRecorded { certificate: b },
        ) => a == b,
        (
            C::ObserveKernelAcceptance { observation: a },
            E::KernelAcceptanceObserved { observation: b },
        ) => a == b,
        (C::Pause { reconciliation: a }, E::Paused { reconciliation: b, .. })
        | (C::Resume { reconciliation: a }, E::Resumed { reconciliation: b, .. }) => a == b,
        (C::Cancel { cause_digest: a }, E::CancellationRequested { cause_digest: b }) => a == b,
        (
            C::ReconcileCancellation { observation: a },
            E::CancellationReconciled { observation: b },
        ) => a == b,
        (C::Reject { cause_digest: a }, E::Rejected { terminal: b })
        | (C::Fail { cause_digest: a }, E::Failed { terminal: b })
        | (C::Exhaust { cause_digest: a }, E::Exhausted { terminal: b }) => *a == b.cause_digest(),
        (C::Finalize, E::Finalized { .. }) => true,
        _ => false,
    }
}
