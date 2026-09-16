//! Input-defined final and unexpected approval predicates.

#[cfg(verus_only)]
use crate::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject, UnmetCondition,
};
#[cfg(verus_only)]
use peritus_spec::{AcceptanceContract, ContentReference, HumanApprovalPolicy};
#[cfg(verus_only)]
use peritus_types::{FindingId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// A current observation authorizes or denies final acceptance.
pub open spec fn current_acceptance_approval_at(
    values: Seq<ApprovalObservation>,
    revision: RevisionTuple,
    index: int,
) -> bool {
    0 <= index < values.len()
        && crate::model::revision_fresh(values[index].spec_revision(), revision)
        && crate::canonical::collections::subjects_match(
            values[index].spec_subject(),
            ApprovalSubject::Acceptance,
        )
}

/// The first current final-acceptance observation in canonical request order.
pub open spec fn first_current_acceptance_approval_at(
    values: Seq<ApprovalObservation>,
    revision: RevisionTuple,
    index: int,
) -> bool {
    current_acceptance_approval_at(values, revision, index)
        && forall |prior: int| 0 <= prior < index ==>
            !current_acceptance_approval_at(values, revision, prior)
}

pub open spec fn first_current_acceptance_has_authority(
    values: Seq<ApprovalObservation>,
    revision: RevisionTuple,
    authority: ContentReference,
) -> bool {
    exists |index: int| #[trigger] first_current_acceptance_approval_at(values, revision, index)
        && crate::model::authority::authority_matches(values[index].spec_authority(), authority)
}

pub open spec fn first_current_acceptance_approved(
    values: Seq<ApprovalObservation>,
    revision: RevisionTuple,
) -> bool {
    exists |index: int| #[trigger] first_current_acceptance_approval_at(values, revision, index)
        && values[index].spec_outcome() == ApprovalOutcome::Approved
}

pub open spec fn current_acceptance_approval_exists(
    values: Seq<ApprovalObservation>,
    revision: RevisionTuple,
) -> bool {
    exists |index: int| #[trigger] first_current_acceptance_approval_at(values, revision, index)
}

/// Exact diagnostic selected by the executable final-approval phase.
pub open spec fn final_approval_failure(
    contract: &AcceptanceContract,
    revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> Option<UnmetCondition> {
    match contract.spec_approval_policy() {
        HumanApprovalPolicy::NotRequired => None,
        HumanApprovalPolicy::Required(authority) => {
            if !current_acceptance_approval_exists(evidence.spec_approvals(), revision) {
                Some(UnmetCondition::MissingHumanApproval)
            } else if !first_current_acceptance_has_authority(
                evidence.spec_approvals(), revision, authority,
            ) {
                Some(UnmetCondition::WrongHumanApprovalAuthority)
            } else if !first_current_acceptance_approved(evidence.spec_approvals(), revision) {
                Some(UnmetCondition::HumanApprovalDenied)
            } else {
                None
            }
        },
    }
}

/// The declared final-approval requirement is satisfied by the actual observations.
pub open spec fn final_approval_complete(
    contract: &AcceptanceContract,
    revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    final_approval_failure(contract, revision, evidence).is_none()
}

pub open spec fn waiver_approval_witness(
    contract: &AcceptanceContract,
    revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: ApprovalObservation,
    finding: FindingId,
    waiver: int,
) -> bool {
    0 <= waiver < evidence.spec_waivers().len()
        && crate::model::authority::current_waiver_at(
            evidence.spec_waivers(), finding, revision, waiver,
        )
        && crate::model::authority::request_ids_match(
            evidence.spec_waivers()[waiver].spec_approval_request_id(),
            approval.spec_request_id(),
        )
        && crate::model::authority::supplied_waiver_valid(
            contract,
            revision,
            evidence,
            evidence.spec_waivers()[waiver],
        )
}

/// A finding-waiver approval is expected exactly when it completes a current authorized waiver.
pub open spec fn waiver_approval_expected(
    contract: &AcceptanceContract,
    revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: ApprovalObservation,
    finding: FindingId,
) -> bool {
    exists |waiver: int|
        #[trigger] waiver_approval_witness(
            contract, revision, evidence, approval, finding, waiver,
        )
}

/// Whether one current approval observation is related to the configured acceptance policy.
pub open spec fn approval_expected(
    contract: &AcceptanceContract,
    revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: ApprovalObservation,
) -> bool {
    match approval.spec_subject() {
        ApprovalSubject::Acceptance => match contract.spec_approval_policy() {
            HumanApprovalPolicy::NotRequired => false,
            HumanApprovalPolicy::Required(_) => true,
        },
        ApprovalSubject::FindingWaiver(finding) => {
            waiver_approval_expected(contract, revision, evidence, approval, finding)
        },
    }
}

/// Every approval for the requested revision has a configured acceptance or waiver purpose.
pub open spec fn unexpected_approvals_complete(
    contract: &AcceptanceContract,
    revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |index: int| 0 <= index < evidence.spec_approvals().len()
        && crate::model::revision_fresh(
            #[trigger] evidence.spec_approvals()[index].spec_revision(), revision,
        )
        ==> approval_expected(contract, revision, evidence, evidence.spec_approvals()[index])
}

} // verus!
