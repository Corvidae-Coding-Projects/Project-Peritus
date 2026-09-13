//! Finding-waiver transitions bound to current B2 evidence.

use super::AppliedCommand;
use crate::{
    AuthorityInputKind, KernelAggregate, KernelCommand, KernelError, KernelErrorKind,
    KernelEventKind, KernelSubject, LifecycleEntity, ReducerInputs, ReviewPhase, WaiverPhase,
    WaiverState,
};
use peritus_quality_policy::{
    ApprovalOutcome, ApprovalSubject, UnmetCondition, evaluate_acceptance,
};
use peritus_types::{FindingId, ReviewCycleId, RunId};
use vstd::prelude::*;

verus! {

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
            state, *finding_id, WaiverPhase::Requested, WaiverPhase::Denied,
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
    Ok(AppliedCommand::new(
        KernelEventKind::WaiverRequested,
        KernelSubject::Waiver(finding_id),
    ))
}

#[allow(
    clippy::collapsible_if,
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
    if state.waivers[index].phase() != WaiverPhase::Requested {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Waiver));
    }
    let Some(evidence) = inputs.acceptance_evidence() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    let mut waiver_request = None;
    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len()
        invariant waiver_index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - waiver_index,
    {
        let waiver = &evidence.waivers()[waiver_index];
        if waiver.finding_id() == finding_id && waiver.revision() == state.revision {
            waiver_request = Some(waiver.approval_request_id());
            break;
        }
        waiver_index += 1;
    }
    let Some(request_id) = waiver_request else {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    let mut approved = false;
    let mut approval_index = 0;
    while approval_index < evidence.approvals().len()
        invariant approval_index <= evidence.spec_approvals().len(),
        decreases evidence.spec_approvals().len() - approval_index,
    {
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
    let decision = evaluate_acceptance(inputs.contract(), state.revision, evidence);
    let mut condition_index = 0;
    while condition_index < decision.unmet_conditions().len()
        invariant condition_index <= decision.spec_unmet_conditions().len(),
        decreases decision.spec_unmet_conditions().len() - condition_index,
    {
        if let UnmetCondition::InvalidWaiver { finding_id: target, .. } =
            decision.unmet_conditions()[condition_index]
        {
            if target == finding_id {
                return Err(KernelError::authority(
                    KernelErrorKind::AuthorityMismatch,
                    AuthorityInputKind::AcceptanceEvidence,
                ));
            }
        }
        condition_index += 1;
    }
    state.waivers[index].set_phase(WaiverPhase::Granted);
    Ok(AppliedCommand::new(
        KernelEventKind::WaiverGranted,
        KernelSubject::Waiver(finding_id),
    ))
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
    Ok(AppliedCommand::new(
        KernelEventKind::WaiverInvalidated,
        KernelSubject::Waiver(finding_id),
    ))
}

} // verus!
