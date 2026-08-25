//! Fixer response, independent confirmation, and external waiver transitions.

use peritus_evidence::EvidenceId;
use peritus_spec::WaiverPolicy;
use peritus_types::{ActorId, EventId, FindingId, ReviewCycleId, Sha256Digest};

use crate::disposition::validate_evidence;
use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::state::mutation;
use crate::{
    DispositionKind, DispositionRecord, FixerResponse, ReviewCyclePhase, ReviewEventKind,
    ReviewRunState,
};

pub(super) fn record_response(
    state: &mut ReviewRunState,
    event_id: EventId,
    finding_id: FindingId,
    response: &FixerResponse,
    waiver_only: bool,
) -> Result<(), ReviewError> {
    response.validate(state.limits())?;
    if response.revision() != state.binding().revision() {
        return Err(reject(ReviewErrorKind::BindingMismatch, "fixer response is stale"));
    }
    let finding = state.finding(finding_id).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "fixer response finding does not exist")
    })?;
    if !state.finding_is_current(finding)
        || finding.superseded_by().is_some()
        || finding.current_disposition() != DispositionKind::Open
        || (waiver_only != matches!(response, FixerResponse::WaiverRequested { .. }))
        || (waiver_only && !finding.blocking())
    {
        return Err(reject(
            ReviewErrorKind::IllegalTransition,
            "fixer/request response is stale, contradictory, or not valid for this command",
        ));
    }
    if let Some(superseding) = response.superseding() {
        let target = state.finding(superseding).ok_or_else(|| {
            reject(ReviewErrorKind::UnknownIdentity, "proposed superseding finding is absent")
        })?;
        if superseding == finding_id
            || !state.finding_is_current(target)
            || target.category() != finding.category()
            || target.superseded_by().is_some()
        {
            return Err(reject(
                ReviewErrorKind::IllegalTransition,
                "supersession proposal is self-referential, stale, or category-mismatched",
            ));
        }
    }
    if waiver_only {
        validate_waiver_request(state, response)?;
    }
    let kind = match response {
        FixerResponse::Fixed { .. } => DispositionKind::Fixed,
        FixerResponse::Disputed { .. } => DispositionKind::Disputed,
        FixerResponse::SupersessionProposed { .. } => DispositionKind::SupersessionProposed,
        FixerResponse::WaiverRequested { .. } => DispositionKind::WaiverRequested,
    };
    let finding = mutation::finding_mut(state, finding_id).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "fixer response finding disappeared")
    })?;
    mutation::push_disposition(
        finding,
        DispositionRecord::from_wire(
            event_id,
            kind,
            Some(response.actor()),
            None,
            response.revision(),
            response.evidence().to_vec(),
            response.superseding(),
            response.approval_request_id(),
            response.authority(),
            response.evidence_requirement_id(),
            response.digest(),
        ),
    );
    Ok(())
}

fn validate_waiver_request(
    state: &ReviewRunState,
    response: &FixerResponse,
) -> Result<(), ReviewError> {
    let (authority, evidence) = match state.binding().waiver_policy() {
        WaiverPolicy::Allowed { authority, evidence } => (authority, evidence),
        WaiverPolicy::Forbidden => {
            return Err(reject(ReviewErrorKind::WaiverInvalid, "contract forbids finding waivers"));
        }
    };
    if response.authority() != Some(authority)
        || response.evidence_requirement_id() != Some(evidence)
        || response.approval_request_id().is_none()
        || state
            .findings()
            .iter()
            .flat_map(crate::Finding::dispositions)
            .any(|record| record.approval_request_id() == response.approval_request_id())
    {
        return Err(reject(
            ReviewErrorKind::WaiverInvalid,
            "waiver request differs from contract authority or reuses an identity",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn confirm(
    state: &mut ReviewRunState,
    event_id: EventId,
    finding_id: FindingId,
    reviewer_cycle: ReviewCycleId,
    pending_response_digest: Sha256Digest,
    evidence: &[EvidenceId],
    confirmation_digest: Sha256Digest,
    pending_kind: DispositionKind,
    confirmed_kind: DispositionKind,
    related: Option<FindingId>,
) -> Result<(), ReviewError> {
    let actor = validate_confirmation(
        state,
        finding_id,
        reviewer_cycle,
        pending_response_digest,
        evidence,
        pending_kind,
        related,
    )?;
    let revision = state.binding().revision();
    let finding = mutation::finding_mut(state, finding_id)
        .ok_or_else(|| reject(ReviewErrorKind::UnknownIdentity, "confirmed finding disappeared"))?;
    mutation::push_disposition(
        finding,
        DispositionRecord::from_wire(
            event_id,
            confirmed_kind,
            Some(actor),
            Some(reviewer_cycle),
            revision,
            evidence.to_vec(),
            related,
            None,
            None,
            None,
            confirmation_digest,
        ),
    );
    Ok(())
}

pub(super) fn validate_confirmation(
    state: &ReviewRunState,
    finding_id: FindingId,
    reviewer_cycle: ReviewCycleId,
    pending_response_digest: Sha256Digest,
    evidence: &[EvidenceId],
    pending_kind: DispositionKind,
    related: Option<FindingId>,
) -> Result<ActorId, ReviewError> {
    validate_evidence(evidence, state.limits())?;
    let finding = state.finding(finding_id).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "confirmed finding does not exist")
    })?;
    let pending = finding.dispositions().last().ok_or_else(|| {
        reject(ReviewErrorKind::IllegalTransition, "finding has no pending response")
    })?;
    let cycle = state.cycle(reviewer_cycle).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "confirming reviewer cycle does not exist")
    })?;
    let reviewer = cycle.assignment().reviewer();
    let pending_actor = pending.actor().ok_or_else(|| {
        reject(ReviewErrorKind::EvidenceInvalid, "pending fixer response lacks its actor identity")
    })?;
    if !state.finding_is_current(finding)
        || !state.cycle_is_current(cycle)
        || !matches!(cycle.phase(), ReviewCyclePhase::Assigned | ReviewCyclePhase::Submitted)
        || pending.kind() != pending_kind
        || pending.record_digest() != pending_response_digest
        || pending.related_finding() != related
        || pending.revision() != state.binding().revision()
        || reviewer.actor_id() == pending_actor
        || !reviewer.independent_from_producer()
        || state.binding().producer_actors().binary_search(&reviewer.actor_id()).is_ok()
    {
        return Err(reject(
            ReviewErrorKind::EvidenceInvalid,
            "reviewer confirmation is stale, contradictory, dependent, or mismatched",
        ));
    }
    Ok(reviewer.actor_id())
}

pub(super) fn observe_waiver(
    state: &mut ReviewRunState,
    event_id: EventId,
    waiver: crate::ObservedWaiver,
) -> Result<ReviewEventKind, ReviewError> {
    let observation = waiver.observation();
    let finding = state
        .finding(observation.finding_id())
        .ok_or_else(|| reject(ReviewErrorKind::UnknownIdentity, "waiver finding does not exist"))?;
    let request = finding.dispositions().last().ok_or_else(|| {
        reject(ReviewErrorKind::WaiverInvalid, "waiver finding has no pending request")
    })?;
    let (authority, evidence) = match state.binding().waiver_policy() {
        WaiverPolicy::Allowed { authority, evidence } => (authority, evidence),
        WaiverPolicy::Forbidden => {
            return Err(reject(ReviewErrorKind::WaiverInvalid, "contract forbids waivers"));
        }
    };
    if !state.finding_is_current(finding)
        || !finding.blocking()
        || request.kind() != DispositionKind::WaiverRequested
        || request.record_digest() != waiver.request_digest()
        || observation.revision() != state.binding().revision()
        || observation.approval_request_id()
            != request.approval_request_id().ok_or_else(|| {
                reject(ReviewErrorKind::WaiverInvalid, "waiver request identity is missing")
            })?
        || observation.authority() != authority
        || request.authority() != Some(authority)
        || observation.evidence_requirement_id() != evidence
        || request.evidence_requirement_id() != Some(evidence)
        || state.waivers().iter().any(|existing| {
            existing.finding_id() == observation.finding_id()
                && existing.revision() == observation.revision()
        })
    {
        return Err(reject(
            ReviewErrorKind::WaiverInvalid,
            "external waiver does not exactly match current contract, request, and revision",
        ));
    }
    if state.waivers().len() >= state.limits().findings() as usize {
        return Err(reject(ReviewErrorKind::LimitExceeded, "waiver history limit exhausted"));
    }
    let finding = mutation::finding_mut(state, observation.finding_id())
        .ok_or_else(|| reject(ReviewErrorKind::UnknownIdentity, "waiver finding disappeared"))?;
    mutation::push_disposition(
        finding,
        DispositionRecord::from_wire(
            event_id,
            DispositionKind::Waived,
            None,
            None,
            observation.revision(),
            Vec::new(),
            None,
            Some(observation.approval_request_id()),
            Some(observation.authority()),
            Some(observation.evidence_requirement_id()),
            observation.waiver_digest(),
        ),
    );
    mutation::push_waiver(state, waiver);
    Ok(ReviewEventKind::WaiverObserved { waiver })
}
