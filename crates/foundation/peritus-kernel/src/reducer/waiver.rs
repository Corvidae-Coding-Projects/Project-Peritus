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
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> (result: Result<AppliedCommand, KernelError>)
    ensures
        match result {
            Ok(applied) => applied.event_kind == KernelEventKind::WaiverGranted ==>
                match command {
                    KernelCommand::GrantWaiver { finding_id } =>
                        applied.subject == KernelSubject::Waiver(*finding_id)
                        && final(state).revision == old(state).revision
                        && super::waiver_grant_authorized(old(state), *finding_id, inputs)
                        && super::waiver_grant_recorded(final(state), *finding_id, inputs),
                    _ => false,
                },
            Err(_) => true,
        },
{
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
) -> (result: Result<AppliedCommand, KernelError>)
    ensures
        final(state).revision == old(state).revision,
        result.is_err() ==> *final(state) == *old(state),
        match result {
            Ok(applied) => applied.event_kind == KernelEventKind::WaiverRequested
                && applied.subject == KernelSubject::Waiver(finding_id),
            Err(_) => true,
        },
{
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
    clippy::too_many_lines,
    reason = "the explicit nested branch stays within the supported Verus execution subset"
)]
fn grant(
    state: &mut KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
) -> (result: Result<AppliedCommand, KernelError>)
    ensures
        final(state).revision == old(state).revision,
        result.is_err() ==> *final(state) == *old(state),
        match result {
            Ok(applied) => applied.event_kind == KernelEventKind::WaiverGranted
                && applied.subject == KernelSubject::Waiver(finding_id)
                && super::waiver_grant_authorized(old(state), finding_id, inputs)
                && super::waiver_grant_recorded(final(state), finding_id, inputs),
            Err(_) => true,
        },
{
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
    proof {
        assert(current_phase == WaiverPhase::Requested);
        state.expose_internal_views();
        assert(state.spec_waivers()[index as int].spec_phase() == WaiverPhase::Requested);
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
    let Some(supplied_waiver_index) = current_supplied_waiver(
        evidence,
        finding_id,
        state.revision,
    ) else {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    let request_id = evidence.waivers()[supplied_waiver_index].approval_request_id();
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
    let Some((_supplied_review_index, _supplied_finding_index)) = current_requested_finding(
        evidence,
        requested_review_id,
        finding_id,
        state.revision,
    ) else {
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
    proof {
        state.expose_internal_views();
        if !peritus_quality_policy::supplied_waiver_valid(
            inputs.spec_contract(),
            state.revision,
            evidence,
            evidence.spec_waivers()[supplied_waiver_index as int],
        ) {
            assert(peritus_quality_policy::invalid_waiver_reported(
                decision.spec_unmet_conditions(),
                finding_id,
            ));
            assert(false);
        }
        super::waiver_correspondence::grant_input_witness(
            inputs,
            evidence,
            state.revision,
            finding_id,
            requested_review_id,
            supplied_waiver_index as int,
            _supplied_review_index as int,
            _supplied_finding_index as int,
        );
        super::waiver_correspondence::grant_authorized_witness(
            state,
            finding_id,
            inputs,
            index as int,
            review_index as int,
        );
    }
    state.waivers[index].set_phase(WaiverPhase::Granted);
    proof {
        state.expose_internal_views();
        super::waiver_correspondence::grant_recorded_witness(
            state,
            finding_id,
            inputs,
            index as int,
            review_index as int,
        );
    }
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
) -> (result: Result<AppliedCommand, KernelError>)
    ensures
        final(state).revision == old(state).revision,
        result.is_err() ==> *final(state) == *old(state),
        match result {
            Ok(applied) => applied.event_kind == event_kind
                && applied.subject == KernelSubject::Waiver(finding_id),
            Err(_) => true,
        },
{
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
) -> (result: Result<AppliedCommand, KernelError>)
    ensures
        final(state).revision == old(state).revision,
        result.is_err() ==> *final(state) == *old(state),
        match result {
            Ok(applied) => applied.event_kind == KernelEventKind::WaiverInvalidated
                && applied.subject == KernelSubject::Waiver(finding_id),
            Err(_) => true,
        },
{
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
