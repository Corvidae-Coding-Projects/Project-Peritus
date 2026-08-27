//! Owner-confined reconstruction and atomic settlement of claimed E0 directives.

mod children;

#[cfg(test)]
mod children_tests;

use peritus_journal::{OutboxId, SqliteJournal};
use peritus_orchestrator::{
    ClaimedDirectiveAcknowledgement, DirectiveDeliveryState, OrchestratorCommand,
    OrchestratorCommandKind, OrchestratorError, OrchestratorErrorKind, OrchestratorEvent,
    OrchestratorEventKind, OrchestratorState, PendingDirective,
    commit_claimed_directive_acknowledgement,
};
use peritus_types::{CommandId, EventId};
use sha2::{Digest, Sha256};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, outbox::OrchestratorDirectiveClaim};

const COMMAND_ID_DOMAIN: &[u8] = b"peritus.g0.orchestrator.directive-ack.command.v1\0";
const EVENT_ID_DOMAIN: &[u8] = b"peritus.g0.orchestrator.directive-ack.event.v1\0";

pub(super) fn settle_claimed_directive(
    journal: &mut SqliteJournal,
    claim: &OrchestratorDirectiveClaim,
) -> Result<(), DaemonError> {
    let published = claimed_directive_state(journal, claim)?;
    let command = acknowledgement_command(&published, claim)?;
    let transition = peritus_orchestrator::decide(&published, &command)
        .map_err(|error| orchestrator_error("reduce claimed directive acknowledgement", error))?;
    let outbox_id = OutboxId::new(*claim.directive().id().as_bytes())
        .map_err(|error| journal_identity_error("bind claimed directive outbox identity", error))?;
    let acknowledgement =
        ClaimedDirectiveAcknowledgement::new(claim.command(), outbox_id, claim.fence())
            .map_err(|error| orchestrator_error("bind claimed directive acknowledgement", error))?;
    commit_claimed_directive_acknowledgement(journal, acknowledgement, &command, &transition)
        .map_err(|error| orchestrator_error("commit claimed directive acknowledgement", error))?;
    Ok(())
}

pub(super) fn deliver_child_directive(
    journal: &mut SqliteJournal,
    claim: &OrchestratorDirectiveClaim,
) -> Result<(), DaemonError> {
    children::deliver_child_directive(journal, claim)
}

fn claimed_directive_state(
    journal: &SqliteJournal,
    claim: &OrchestratorDirectiveClaim,
) -> Result<OrchestratorState, DaemonError> {
    let replay = peritus_orchestrator::load_orchestrator_replay(journal, claim.command().run_id())
        .map_err(|error| orchestrator_error("load claimed orchestrator directive", error))?;
    replay
        .rebuild()
        .map_err(|error| orchestrator_error("rebuild claimed orchestrator directive", error))?
        .ok_or_else(|| corrupt("claimed orchestrator run has no durable state"))?;
    let published_index = replay
        .events()
        .iter()
        .position(|event| event_matches_claim(event, claim))
        .ok_or_else(|| corrupt("claimed publication event is absent from the E0 history"))?;
    let published = peritus_orchestrator::replay(&replay.events()[..=published_index])
        .map_err(|error| orchestrator_error("replay claimed publication prefix", error))?;
    let pending = published
        .pending_directive()
        .ok_or_else(|| corrupt("claimed publication has no pending E0 directive"))?;
    if !pending_matches_claim(pending, claim.directive()) {
        return Err(corrupt("pending E0 directive differs from the exact claimed publication"));
    }
    Ok(published)
}

fn acknowledgement_command(
    state: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
) -> Result<OrchestratorCommand, DaemonError> {
    let run_id = claim.command().run_id();
    let directive_id = claim.directive().id();
    let command_id = CommandId::new(stable_identity(
        COMMAND_ID_DOMAIN,
        run_id.as_bytes(),
        directive_id.as_bytes(),
    ))
    .map_err(|error| identity_error("derive directive acknowledgement command identity", error))?;
    let event_id =
        EventId::new(stable_identity(EVENT_ID_DOMAIN, run_id.as_bytes(), directive_id.as_bytes()))
            .map_err(|error| {
                identity_error("derive directive acknowledgement event identity", error)
            })?;
    OrchestratorCommand::new(
        command_id,
        event_id,
        run_id,
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.current_candidate().revision(),
        OrchestratorCommandKind::AcknowledgeDirective { directive_id },
    )
    .map_err(|error| orchestrator_error("construct directive acknowledgement command", error))
}

fn stable_identity(domain: &[u8], run_id: &[u8; 16], directive_id: &[u8; 16]) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(run_id);
    hasher.update(directive_id);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    identity[0] |= 0x80;
    identity
}

fn event_matches_claim(event: &OrchestratorEvent, claim: &OrchestratorDirectiveClaim) -> bool {
    let command = claim.command();
    let envelope_matches = [
        event.id() == command.event_id(),
        event.command_id() == command.command_id(),
        event.run_id() == command.run_id(),
        command.expected_sequence().checked_add(1) == Some(event.sequence().get()),
        event.previous_event() == command.expected_previous_event(),
        event.revision() == command.revision(),
        event.prior_state_digest() == command.prior_state_digest(),
    ]
    .into_iter()
    .all(core::convert::identity);
    let directive_matches = matches!(
        event.kind(),
        OrchestratorEventKind::DirectivePublished { directive }
            if directive == claim.directive()
    );
    envelope_matches && directive_matches
}

fn pending_matches_claim(pending: &PendingDirective, claimed: &PendingDirective) -> bool {
    claimed.delivery_state() == DirectiveDeliveryState::Ready
        && claimed.deliveries() == 0
        && pending.delivery_state() == DirectiveDeliveryState::Published
        && pending.deliveries() == 1
        && pending.id() == claimed.id()
        && pending.destination() == claimed.destination()
        && pending.kind() == claimed.kind()
        && pending.payload_digest() == claimed.payload_digest()
        && pending.maximum_deliveries() == claimed.maximum_deliveries()
        && pending.source_event() == claimed.source_event()
        && pending.task_id() == claimed.task_id()
        && pending.work_id() == claimed.work_id()
        && pending.revision() == claimed.revision()
}

fn orchestrator_error(operation: &'static str, error: OrchestratorError) -> DaemonError {
    let (code, recovery) = match error.kind() {
        OrchestratorErrorKind::StaleState
        | OrchestratorErrorKind::ChildAmbiguous
        | OrchestratorErrorKind::External => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
        OrchestratorErrorKind::InvalidInput
        | OrchestratorErrorKind::InvalidTransition
        | OrchestratorErrorKind::BindingMismatch
        | OrchestratorErrorKind::LimitExceeded
        | OrchestratorErrorKind::NonCanonical
        | OrchestratorErrorKind::Codec
        | OrchestratorErrorKind::Conflict
        | OrchestratorErrorKind::MissingCheckpoint
        | OrchestratorErrorKind::Integrity => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
    };
    DaemonError::with_source(code, recovery, operation, error.detail(), error)
}

fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "settle claimed orchestrator directive",
        detail,
    )
}

fn journal_identity_error(
    operation: &'static str,
    error: peritus_journal::JournalError,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        operation,
        "claimed directive identity cannot form a C0 outbox identity",
        error,
    )
}

fn identity_error(operation: &'static str, _error: peritus_types::IdentifierError) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        operation,
        "domain-separated directive acknowledgement identity is invalid",
    )
}
