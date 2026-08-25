//! Cross-record semantic binding checks before C0 append planning.

use crate::{ReviewCommand, ReviewCommandKind, ReviewError, ReviewEventKind, ReviewTransition};

pub fn validate_binding(
    command: &ReviewCommand,
    transition: &ReviewTransition,
) -> Result<(), ReviewError> {
    let event = transition.event();
    let state = transition.state();
    let mismatches = [
        command.event_id() != event.id(),
        command.command_id() != event.command_id(),
        command.run_id() != event.run_id(),
        command.run_id() != state.run_id(),
        command.expected_previous_event() != event.previous_event(),
        command.expected_sequence().checked_add(1) != Some(event.sequence().get()),
        command.revision() != event.revision(),
        state.binding().revision() != event_revision_after(event.kind(), event.revision()),
        command.prior_state_digest() != event.prior_state_digest(),
        event.successor_state_digest() != state.state_digest(),
        event.sequence() != state.sequence(),
        event.id() != state.last_event_id(),
        !event_matches_command(command.kind(), event.kind()),
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        return Err(super::binding_error(
            "review command, event, and checkpoint do not describe one transition",
        ));
    }
    Ok(())
}

const fn event_revision_after(
    kind: &ReviewEventKind,
    event_revision: peritus_types::RevisionTuple,
) -> peritus_types::RevisionTuple {
    match kind {
        ReviewEventKind::RevisionAdvanced { binding } => binding.revision(),
        _ => event_revision,
    }
}

#[allow(clippy::too_many_lines)]
fn event_matches_command(command: &ReviewCommandKind, event: &ReviewEventKind) -> bool {
    match (command, event) {
        (
            ReviewCommandKind::StartRun { binding: left, limits: left_limits },
            ReviewEventKind::RunStarted { binding: right, limits: right_limits },
        ) => left == right && left_limits == right_limits,
        (
            ReviewCommandKind::AdvanceRevision { binding: left },
            ReviewEventKind::RevisionAdvanced { binding: right },
        ) => left == right,
        (
            ReviewCommandKind::AssignReviewer { assignment: left },
            ReviewEventKind::ReviewerAssigned { assignment: right },
        ) => left == right,
        (
            ReviewCommandKind::SubmitReview { submission: left },
            ReviewEventKind::ReviewSubmitted { submission: right },
        ) => left == right,
        (
            ReviewCommandKind::ReconcileDuplicates {
                canonical: left_id,
                duplicates: left_duplicates,
                reconciliation_digest: left_digest,
            },
            ReviewEventKind::DuplicatesReconciled {
                canonical: right_id,
                duplicates: right_duplicates,
                reconciliation_digest: right_digest,
            },
        ) => {
            left_id == right_id
                && left_duplicates == right_duplicates
                && left_digest == right_digest
        }
        (
            ReviewCommandKind::RecordFixerResponse { finding_id: left_id, response: left },
            ReviewEventKind::FixerResponseRecorded { finding_id: right_id, response: right },
        )
        | (
            ReviewCommandKind::RequestWaiver { finding_id: left_id, request: left },
            ReviewEventKind::WaiverRequested { finding_id: right_id, request: right },
        ) => left_id == right_id && left == right,
        (
            ReviewCommandKind::ConfirmResolution {
                finding_id: left_id,
                reviewer_cycle: left_cycle,
                pending_response_digest: left_pending,
                evidence: left_evidence,
                confirmation_digest: left_digest,
            },
            ReviewEventKind::ResolutionConfirmed {
                finding_id: right_id,
                reviewer_cycle: right_cycle,
                pending_response_digest: right_pending,
                evidence: right_evidence,
                confirmation_digest: right_digest,
            },
        )
        | (
            ReviewCommandKind::ConfirmInvalidation {
                finding_id: left_id,
                reviewer_cycle: left_cycle,
                pending_response_digest: left_pending,
                evidence: left_evidence,
                confirmation_digest: left_digest,
            },
            ReviewEventKind::InvalidationConfirmed {
                finding_id: right_id,
                reviewer_cycle: right_cycle,
                pending_response_digest: right_pending,
                evidence: right_evidence,
                confirmation_digest: right_digest,
            },
        ) => {
            left_id == right_id
                && left_cycle == right_cycle
                && left_pending == right_pending
                && left_evidence == right_evidence
                && left_digest == right_digest
        }
        (
            ReviewCommandKind::ConfirmSupersession {
                finding_id: left_id,
                superseding: left_superseding,
                reviewer_cycle: left_cycle,
                pending_response_digest: left_pending,
                evidence: left_evidence,
                confirmation_digest: left_digest,
            },
            ReviewEventKind::SupersessionConfirmed {
                finding_id: right_id,
                superseding: right_superseding,
                reviewer_cycle: right_cycle,
                pending_response_digest: right_pending,
                evidence: right_evidence,
                confirmation_digest: right_digest,
            },
        ) => {
            left_id == right_id
                && left_superseding == right_superseding
                && left_cycle == right_cycle
                && left_pending == right_pending
                && left_evidence == right_evidence
                && left_digest == right_digest
        }
        (
            ReviewCommandKind::ObserveWaiver { waiver: left },
            ReviewEventKind::WaiverObserved { waiver: right },
        ) => left == right,
        (
            ReviewCommandKind::CancelCycle { cycle_id: left },
            ReviewEventKind::CycleCancelled { cycle_id: right },
        ) => left == right,
        (ReviewCommandKind::CancelRun, ReviewEventKind::RunCancelled)
        | (ReviewCommandKind::FinalizeRun, ReviewEventKind::RunFinalized) => true,
        (
            ReviewCommandKind::ExhaustBudget { reason_digest: left },
            ReviewEventKind::BudgetExhausted { reason_digest: right },
        )
        | (
            ReviewCommandKind::FailRun { failure_digest: left },
            ReviewEventKind::RunFailed { failure_digest: right },
        ) => left == right,
        _ => false,
    }
}
