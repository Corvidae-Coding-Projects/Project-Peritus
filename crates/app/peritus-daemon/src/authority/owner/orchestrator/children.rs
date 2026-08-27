//! Durable child admission before exact E0/C0 claim settlement.

mod collaboration;
mod gates_review;
mod scheduler;

#[cfg(test)]
pub(super) use collaboration::commit_collaboration_directive;
#[cfg(test)]
pub(super) use gates_review::commit_review_lifecycle;
#[cfg(test)]
pub(super) use scheduler::commit_scheduler_directive;

use peritus_journal::SqliteJournal;
use peritus_orchestrator::{
    ChildAggregateKind, ChildHead, DirectiveDestination, DirectiveKind, DirectivePayloadBinding,
    OrchestratorState, directive_payload_digest,
};
use peritus_types::{CommandId, EventId, RunId, Sha256Digest};

use super::{claimed_directive_state, identity_error, settle_claimed_directive, stable_identity};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, outbox::OrchestratorDirectiveClaim};

pub(super) fn deliver_child_directive(
    journal: &mut SqliteJournal,
    claim: &OrchestratorDirectiveClaim,
) -> Result<(), DaemonError> {
    let orchestrator = claimed_directive_state(journal, claim)?;
    match claim.directive().destination() {
        DirectiveDestination::Scheduler => {
            scheduler::admit_scheduler_directive(journal, &orchestrator, claim)?;
        }
        DirectiveDestination::Collaboration => {
            collaboration::admit_collaboration_directive(journal, &orchestrator, claim)?;
        }
        DirectiveDestination::Gates | DirectiveDestination::Review => {
            gates_review::admit_lifecycle_directive(journal, &orchestrator, claim)?;
        }
        DirectiveDestination::Agent
        | DirectiveDestination::QualityEvaluator
        | DirectiveDestination::Kernel => {
            return Err(child_mismatch(
                "native child handler received a non-child-owned destination",
            ));
        }
    }
    settle_claimed_directive(journal, claim)
}

fn checked_lifecycle_head(
    state: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
) -> Result<ChildHead, DaemonError> {
    let directive = claim.directive();
    if !matches!(directive.kind(), DirectiveKind::PauseChildren | DirectiveKind::ResumeChildren) {
        return Err(child_mismatch("E0 child lifecycle directive is not pause or resume"));
    }
    let reconciliation = state.paused_reconciliation().ok_or_else(|| {
        child_mismatch("E0 child lifecycle directive has no retained reconciliation")
    })?;
    checked_payload(
        claim,
        DirectivePayloadBinding::Reconciliation(reconciliation),
        "E0 child directive payload differs from the retained reconciliation",
    )?;
    let aggregate = match directive.destination() {
        DirectiveDestination::Scheduler => ChildAggregateKind::Scheduler,
        DirectiveDestination::Collaboration => ChildAggregateKind::Collaboration,
        DirectiveDestination::Gates => ChildAggregateKind::Gates,
        DirectiveDestination::Review => ChildAggregateKind::Review,
        DirectiveDestination::Agent
        | DirectiveDestination::QualityEvaluator
        | DirectiveDestination::Kernel => {
            return Err(child_mismatch("child lifecycle destination is unsupported"));
        }
    };
    reconciliation
        .child_heads()
        .iter()
        .copied()
        .find(|head| head.aggregate() == aggregate)
        .ok_or_else(|| child_mismatch("E0 reconciliation has no exact destination child head"))
}

fn checked_payload(
    claim: &OrchestratorDirectiveClaim,
    binding: DirectivePayloadBinding<'_>,
    mismatch: &'static str,
) -> Result<(), DaemonError> {
    let directive = claim.directive();
    let expected = directive_payload_digest(directive.kind(), directive.destination(), binding)
        .map_err(|error| super::orchestrator_error("verify child directive payload", error))?;
    if directive.payload_digest() == expected { Ok(()) } else { Err(child_mismatch(mismatch)) }
}

#[derive(Clone, Copy)]
struct ChildPredecessor {
    sequence: u64,
    event_id: EventId,
    state_digest: Sha256Digest,
}

pub(super) fn child_ids(
    command_domain: &[u8],
    event_domain: &[u8],
    run_id: RunId,
    directive_id: peritus_orchestrator::DirectiveId,
    operation: &'static str,
) -> Result<(CommandId, EventId), DaemonError> {
    let command =
        CommandId::new(stable_identity(command_domain, run_id.as_bytes(), directive_id.as_bytes()))
            .map_err(|error| identity_error(operation, error))?;
    let event =
        EventId::new(stable_identity(event_domain, run_id.as_bytes(), directive_id.as_bytes()))
            .map_err(|error| identity_error(operation, error))?;
    Ok((command, event))
}

fn child_mismatch(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "deliver E0 child directive",
        detail,
    )
}

fn unsupported_child(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Unsupported,
        DaemonRecovery::Operator,
        "deliver E0 child directive",
        detail,
    )
}
