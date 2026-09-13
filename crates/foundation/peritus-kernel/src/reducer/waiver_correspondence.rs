//! Input-defined authority predicates for recorded finding-waiver grants.

use crate::{
    KernelAggregate, KernelCommand, KernelEventKind, KernelOutcome, KernelSubject, ReducerInputs,
};
use peritus_quality_policy::AcceptanceEvidence;
use peritus_types::{FindingId, ReviewCycleId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// Exact stored request and supplied policy evidence required to grant one finding waiver.
pub open spec fn waiver_grant_authorized(
    before: &KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
) -> bool {
    exists |waiver_index: int, review_index: int|
        0 <= waiver_index < before.spec_waivers().len()
        && peritus_quality_policy::finding_ids_match(
            before.spec_waivers()[waiver_index].spec_finding_id(), finding_id)
        && before.spec_waivers()[waiver_index].spec_phase() == crate::WaiverPhase::Requested
        && 0 <= review_index < before.spec_reviews().len()
        && before.spec_reviews()[review_index].spec_id().spec_bytes()@
            == before.spec_waivers()[waiver_index].spec_review_cycle_id().spec_bytes()@
        && before.spec_reviews()[review_index].spec_run_id().spec_bytes()@
            == before.spec_waivers()[waiver_index].spec_run_id().spec_bytes()@
        && inputs.spec_waiver_grant_authorized(
            before.spec_revision(),
            finding_id,
            before.spec_waivers()[waiver_index].spec_review_cycle_id(),
        )
}

/// A recorded grant retains its stored review/run binding and exact supplied policy authority.
pub open spec fn waiver_grant_recorded(
    after: &KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
) -> bool {
    exists |waiver_index: int, review_index: int|
        0 <= waiver_index < after.spec_waivers().len()
        && peritus_quality_policy::finding_ids_match(
            after.spec_waivers()[waiver_index].spec_finding_id(), finding_id)
        && after.spec_waivers()[waiver_index].spec_phase() == crate::WaiverPhase::Granted
        && 0 <= review_index < after.spec_reviews().len()
        && after.spec_reviews()[review_index].spec_id().spec_bytes()@
            == after.spec_waivers()[waiver_index].spec_review_cycle_id().spec_bytes()@
        && after.spec_reviews()[review_index].spec_run_id().spec_bytes()@
            == after.spec_waivers()[waiver_index].spec_run_id().spec_bytes()@
        && inputs.spec_waiver_grant_authorized(
            after.spec_revision(),
            finding_id,
            after.spec_waivers()[waiver_index].spec_review_cycle_id(),
        )
}

/// A successful grant command records only the exact supplied authority and parent binding.
/// Other command families retain their separate lifecycle refinement obligations.
pub open spec fn waiver_grant_result_authorized(
    before: &KernelAggregate,
    command: KernelCommand,
    inputs: &ReducerInputs<'_>,
    result: KernelOutcome,
) -> bool {
    match result {
        KernelOutcome::Applied(transition) => match command {
            KernelCommand::GrantWaiver { finding_id } =>
                transition.spec_event().spec_kind() == KernelEventKind::WaiverGranted ==>
                    transition.spec_event().spec_subject() == KernelSubject::Waiver(finding_id)
                    && waiver_grant_authorized(before, finding_id, inputs)
                    && waiver_grant_recorded(
                        &transition.spec_aggregate(), finding_id, inputs),
            _ => true,
        },
        KernelOutcome::Rejected { .. } => true,
    }
}

pub(super) proof fn grant_input_witness<'a>(
    inputs: &ReducerInputs<'a>,
    evidence: &'a AcceptanceEvidence,
    revision: RevisionTuple,
    finding_id: FindingId,
    review_id: ReviewCycleId,
    waiver_index: int,
    review_index: int,
    finding_index: int,
)
    requires
        inputs.spec_acceptance_evidence() == Some(evidence),
        peritus_quality_policy::current_waiver_at(
            evidence.spec_waivers(), finding_id, revision, waiver_index),
        peritus_quality_policy::supplied_waiver_valid(
            inputs.spec_contract(), revision, evidence, evidence.spec_waivers()[waiver_index]),
        peritus_quality_policy::current_finding_at(
            evidence.spec_reviews(), finding_id, revision, review_index, finding_index),
        evidence.spec_reviews()[review_index].spec_cycle_id().spec_bytes()@
            == review_id.spec_bytes()@,
    ensures inputs.spec_waiver_grant_authorized(revision, finding_id, review_id),
{
    inputs.establish_waiver_grant_authorized(
        evidence,
        revision,
        finding_id,
        review_id,
        waiver_index,
        review_index,
        finding_index,
    );
}

pub(super) proof fn grant_authorized_witness(
    before: &KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
    waiver_index: int,
    review_index: int,
)
    requires
        0 <= waiver_index < before.spec_waivers().len(),
        peritus_quality_policy::finding_ids_match(
            before.spec_waivers()[waiver_index].spec_finding_id(), finding_id),
        before.spec_waivers()[waiver_index].spec_phase() == crate::WaiverPhase::Requested,
        0 <= review_index < before.spec_reviews().len(),
        before.spec_reviews()[review_index].spec_id().spec_bytes()@
            == before.spec_waivers()[waiver_index].spec_review_cycle_id().spec_bytes()@,
        before.spec_reviews()[review_index].spec_run_id().spec_bytes()@
            == before.spec_waivers()[waiver_index].spec_run_id().spec_bytes()@,
        inputs.spec_waiver_grant_authorized(
            before.spec_revision(),
            finding_id,
            before.spec_waivers()[waiver_index].spec_review_cycle_id(),
        ),
    ensures waiver_grant_authorized(before, finding_id, inputs),
{}

pub(super) proof fn grant_recorded_witness(
    after: &KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
    waiver_index: int,
    review_index: int,
)
    requires
        0 <= waiver_index < after.spec_waivers().len(),
        peritus_quality_policy::finding_ids_match(
            after.spec_waivers()[waiver_index].spec_finding_id(), finding_id),
        after.spec_waivers()[waiver_index].spec_phase() == crate::WaiverPhase::Granted,
        0 <= review_index < after.spec_reviews().len(),
        after.spec_reviews()[review_index].spec_id().spec_bytes()@
            == after.spec_waivers()[waiver_index].spec_review_cycle_id().spec_bytes()@,
        after.spec_reviews()[review_index].spec_run_id().spec_bytes()@
            == after.spec_waivers()[waiver_index].spec_run_id().spec_bytes()@,
        inputs.spec_waiver_grant_authorized(
            after.spec_revision(),
            finding_id,
            after.spec_waivers()[waiver_index].spec_review_cycle_id(),
        ),
    ensures waiver_grant_recorded(after, finding_id, inputs),
{}

pub(super) proof fn grant_recorded_is_extensional(
    established: &KernelAggregate,
    current: &KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
)
    requires
        waiver_grant_recorded(established, finding_id, inputs),
        current.spec_revision() == established.spec_revision(),
        current.spec_reviews() == established.spec_reviews(),
        current.spec_waivers() == established.spec_waivers(),
    ensures waiver_grant_recorded(current, finding_id, inputs),
{}

pub(super) proof fn grant_authorized_is_extensional(
    established: &KernelAggregate,
    current: &KernelAggregate,
    finding_id: FindingId,
    inputs: &ReducerInputs<'_>,
)
    requires
        waiver_grant_authorized(established, finding_id, inputs),
        current.spec_revision() == established.spec_revision(),
        current.spec_reviews() == established.spec_reviews(),
        current.spec_waivers() == established.spec_waivers(),
    ensures waiver_grant_authorized(current, finding_id, inputs),
{}

pub(super) proof fn grant_revision_follows_clone(
    apply_input: &KernelAggregate,
    granted: &KernelAggregate,
    original: &KernelAggregate,
)
    requires
        granted.revision == apply_input.revision,
        apply_input.spec_revision() == original.spec_revision(),
    ensures granted.spec_revision() == original.spec_revision(),
{
    apply_input.expose_internal_views();
    granted.expose_internal_views();
}

} // verus!
