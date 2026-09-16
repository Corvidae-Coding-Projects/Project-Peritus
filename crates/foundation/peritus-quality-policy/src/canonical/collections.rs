//! Independent canonical admission predicates for the five evidence collections.

#[cfg(verus_only)]
use crate::{ApprovalObservation, ApprovalSubject, EvidenceObservation, GateObservation, ReviewObservation, WaiverObservation};
use vstd::prelude::*;

verus! {

pub open spec fn gate_keys(values: Seq<GateObservation>) -> Seq<Seq<u8>> {
    values.map(|i: int, value: GateObservation| value.spec_gate_id().spec_bytes()@)
}
pub open spec fn artifact_keys(values: Seq<EvidenceObservation>) -> Seq<Seq<u8>> {
    values.map(|i: int, value: EvidenceObservation| value.spec_requirement_id().spec_digest().spec_bytes()@)
}
pub open spec fn review_keys(values: Seq<ReviewObservation>) -> Seq<Seq<u8>> {
    values.map(|i: int, value: ReviewObservation| value.spec_cycle_id().spec_bytes()@)
}
pub open spec fn approval_keys(values: Seq<ApprovalObservation>) -> Seq<Seq<u8>> {
    values.map(|i: int, value: ApprovalObservation| value.spec_request_id().spec_bytes()@)
}
pub open spec fn waiver_keys(values: Seq<WaiverObservation>) -> Seq<Seq<u8>> {
    values.map(|i: int, value: WaiverObservation| value.spec_finding_id().spec_bytes()@)
}

/// Equality of the complete semantic approval subject, including all finding-identity bytes.
pub open spec fn subjects_match(left: ApprovalSubject, right: ApprovalSubject) -> bool {
    match (left, right) {
        (ApprovalSubject::Acceptance, ApprovalSubject::Acceptance) => true,
        (ApprovalSubject::FindingWaiver(left), ApprovalSubject::FindingWaiver(right)) => left.spec_bytes()@ == right.spec_bytes()@,
        _ => false,
    }
}

pub open spec fn finding_seen_before(values: Seq<ReviewObservation>, end: int, id: Seq<u8>) -> bool {
    exists |review: int, finding: int| 0 <= review < end && review < values.len()
        && 0 <= finding < values[review].spec_findings().len()
        && #[trigger] values[review].spec_findings()[finding].spec_finding_id().spec_bytes()@ == id
}

pub open spec fn review_findings_unique_at(values: Seq<ReviewObservation>, index: int) -> bool {
    0 <= index < values.len() && forall |finding: int| 0 <= finding < values[index].spec_findings().len() ==>
        !finding_seen_before(values, index, #[trigger] values[index].spec_findings()[finding].spec_finding_id().spec_bytes()@)
}

/// Both review identities and cycle ordinals increase; finding identities are unique across reviews.
pub open spec fn reviews_canonical(values: Seq<ReviewObservation>) -> bool {
    super::order::ordered(review_keys(values))
        && (forall |index: int| 1 <= index < values.len() ==>
            #[trigger] values[index - 1].spec_cycle_ordinal() < values[index].spec_cycle_ordinal())
        && (forall |index: int| 0 <= index < values.len() ==>
            review_findings_unique_at(values, index))
        && (forall |index: int| 0 <= index < values.len() ==>
            #[trigger] values[index].spec_is_canonical())
}

pub open spec fn subject_seen_before(values: Seq<ApprovalObservation>, end: int, subject: ApprovalSubject) -> bool {
    exists |index: int| 0 <= index < end && index < values.len()
        && subjects_match(#[trigger] values[index].spec_subject(), subject)
}

/// Request identities ascend and no semantic subject appears more than once.
pub open spec fn approvals_canonical(values: Seq<ApprovalObservation>) -> bool {
    super::order::ordered(approval_keys(values))
        && forall |index: int| 0 <= index < values.len() ==>
            !subject_seen_before(values, index, #[trigger] values[index].spec_subject())
}

/// A referenced approval, when supplied, targets the same finding. Missing approval is allowed here.
pub open spec fn waiver_approval_consistent(waiver: WaiverObservation, approvals: Seq<ApprovalObservation>) -> bool {
    forall |index: int| 0 <= index < approvals.len()
        && #[trigger] approvals[index].spec_request_id().spec_bytes()@ == waiver.spec_approval_request_id().spec_bytes()@
        ==> subjects_match(approvals[index].spec_subject(), ApprovalSubject::FindingWaiver(waiver.spec_finding_id()))
}

pub open spec fn waivers_canonical(values: Seq<WaiverObservation>, approvals: Seq<ApprovalObservation>) -> bool {
    super::order::ordered(waiver_keys(values))
        && forall |index: int| 0 <= index < values.len() ==>
            waiver_approval_consistent(#[trigger] values[index], approvals)
}

/// Exact constructor admission, without requirements on absent policy evidence.
pub open spec fn evidence_admissible(
    gates: Seq<GateObservation>, reviews: Seq<ReviewObservation>, artifacts: Seq<EvidenceObservation>,
    approvals: Seq<ApprovalObservation>, waivers: Seq<WaiverObservation>,
) -> bool {
    super::order::ordered(gate_keys(gates)) && reviews_canonical(reviews)
        && super::order::ordered(artifact_keys(artifacts)) && approvals_canonical(approvals)
        && waivers_canonical(waivers, approvals)
}

/// Global uniqueness of every collection's primary identity, including nonadjacent positions.
pub open spec fn identities_unique(
    gates: Seq<GateObservation>, reviews: Seq<ReviewObservation>, artifacts: Seq<EvidenceObservation>,
    approvals: Seq<ApprovalObservation>, waivers: Seq<WaiverObservation>,
) -> bool {
    super::order::unique(gate_keys(gates)) && super::order::unique(review_keys(reviews))
        && super::order::unique(artifact_keys(artifacts)) && super::order::unique(approval_keys(approvals))
        && super::order::unique(waiver_keys(waivers))
}

pub proof fn admissible_implies_unique(
    gates: Seq<GateObservation>, reviews: Seq<ReviewObservation>, artifacts: Seq<EvidenceObservation>,
    approvals: Seq<ApprovalObservation>, waivers: Seq<WaiverObservation>,
)
    requires evidence_admissible(gates, reviews, artifacts, approvals, waivers),
    ensures identities_unique(gates, reviews, artifacts, approvals, waivers),
{
    super::order::ordered_implies_unique(gate_keys(gates), 16);
    super::order::ordered_implies_unique(review_keys(reviews), 16);
    super::order::ordered_implies_unique(artifact_keys(artifacts), 32);
    super::order::ordered_implies_unique(approval_keys(approvals), 16);
    super::order::ordered_implies_unique(waiver_keys(waivers), 16);
}

} // verus!
