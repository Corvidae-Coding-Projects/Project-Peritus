//! Finding-waiver transitions bound to current B2 evidence.

use super::{
    AppliedCommand,
    waiver_evidence::{
        current_requested_finding, current_supplied_waiver, invalid_waiver_is_reported,
    },
};
use crate::{
    AuthorityInputKind, KernelAggregate, KernelCommand, KernelError, KernelErrorKind,
    KernelEventKind, KernelSubject, LifecycleEntity, ReducerInputs, ReviewPhase, WaiverPhase,
    WaiverState,
};
use peritus_quality_policy::{ApprovalOutcome, ApprovalSubject, evaluate_acceptance};
use peritus_types::{FindingId, ReviewCycleId, RunId};

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::RequestWaiver { run_id, review_id, finding_id } => {
            request(state, *run_id, *review_id, *finding_id)
        }
        KernelCommand::GrantWaiver { finding_id } => grant(state, *finding_id, inputs),
        KernelCommand::DenyWaiver { finding_id } => phase(
            state,
            *finding_id,
            WaiverPhase::Requested,
            WaiverPhase::Denied,
            KernelEventKind::WaiverDenied,
        ),
        KernelCommand::InvalidateWaiver { finding_id } => invalidate(state, *finding_id),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Waiver)),
    }
}
fn request(
    state: &mut KernelAggregate,
    run_id: RunId,
    review_id: ReviewCycleId,
    finding_id: FindingId,
) -> Result<AppliedCommand, KernelError> {
    let Some(review_index) = state.review_index(review_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Review));
    };
    if state.reviews[review_index].run_id() != run_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Review));
    }
    if state.reviews[review_index].phase() != ReviewPhase::Submitted {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Review));
    }
    if state.waiver(finding_id).is_some() {
        return Err(KernelError::entity(KernelErrorKind::DuplicateEntity, LifecycleEntity::Waiver));
    }
    state.waivers.push(WaiverState::requested(finding_id, review_id, run_id));
    Ok(AppliedCommand::new(KernelEventKind::WaiverRequested, KernelSubject::Waiver(finding_id)))
}
#[allow(
    clippy::collapsible_if,
    clippy::too_many_lines,
    reason = "the explicit nested branch stays within the supported Verus execution subset"
)]
fn grant(
    state: &mut KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.waiver_index(finding_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Waiver));
    };
    let current_phase = state.waivers[index].phase();
    match current_phase {
        WaiverPhase::Requested => {}
        _ => {
            return Err(KernelError::entity(
                KernelErrorKind::IllegalPhase,
                LifecycleEntity::Waiver,
            ));
        }
    }
    let requested_review_id = state.waivers[index].review_cycle_id();
    let requested_run_id = state.waivers[index].run_id();
    let Some(review_index) = state.review_index(requested_review_id) else {
        return Err(KernelError::new(KernelErrorKind::InvalidAggregate));
    };
    if !crate::identity::run_id_equal(state.reviews[review_index].run_id(), requested_run_id) {
        return Err(KernelError::new(KernelErrorKind::InvalidAggregate));
    }
    let Some(evidence) = inputs.acceptance_evidence() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    let Some(supplied_waiver_index) = current_supplied_waiver(evidence, finding_id, state.revision)
    else {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    let request_id = evidence.waivers()[supplied_waiver_index].approval_request_id();
    let mut approved = false;
    let mut approval_index = 0;
    while approval_index < evidence.approvals().len() {
        let approval = &evidence.approvals()[approval_index];
        if approval.request_id() == request_id
            && approval.revision() == state.revision
            && approval.subject() == ApprovalSubject::FindingWaiver(finding_id)
            && approval.outcome() == ApprovalOutcome::Approved
        {
            approved = true;
            break;
        }
        approval_index += 1;
    }
    if !approved {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    }
    let Some((_supplied_review_index, _supplied_finding_index)) =
        current_requested_finding(evidence, requested_review_id, finding_id, state.revision)
    else {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    let decision = evaluate_acceptance(inputs.contract(), state.revision, evidence);
    if invalid_waiver_is_reported(decision.unmet_conditions(), finding_id) {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    }
    state.waivers[index].set_phase(WaiverPhase::Granted);
    Ok(AppliedCommand::new(KernelEventKind::WaiverGranted, KernelSubject::Waiver(finding_id)))
}
fn phase(
    state: &mut KernelAggregate,
    finding_id: FindingId,
    expected: WaiverPhase,
    next: WaiverPhase,
    event_kind: KernelEventKind,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.waiver_index(finding_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Waiver));
    };
    if state.waivers[index].phase() != expected {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Waiver));
    }
    state.waivers[index].set_phase(next);
    Ok(AppliedCommand::new(event_kind, KernelSubject::Waiver(finding_id)))
}
fn invalidate(
    state: &mut KernelAggregate,
    finding_id: FindingId,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.waiver_index(finding_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Waiver));
    };
    if state.waivers[index].phase() == WaiverPhase::Invalidated {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Waiver));
    }
    state.waivers[index].set_phase(WaiverPhase::Invalidated);
    Ok(AppliedCommand::new(KernelEventKind::WaiverInvalidated, KernelSubject::Waiver(finding_id)))
}
