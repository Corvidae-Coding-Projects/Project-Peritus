//! Blocker, waiver, and final human-approval evaluation.

mod blockers;
pub(super) mod diagnostics;
mod lookup;
mod supplied;
mod waiver;

use crate::{AcceptanceEvidence, ApprovalOutcome, ApprovalSubject, UnmetCondition};
#[cfg(verus_only)]
use crate::model::approvals::*;
use peritus_spec::{AcceptanceContract, HumanApprovalPolicy};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

pub(super) fn evaluate_waivers(
    contract: &AcceptanceContract, requested: RevisionTuple,
    evidence: &AcceptanceEvidence, unmet: &mut Vec<UnmetCondition>,
) -> (complete: bool)
    ensures
        complete == crate::model::authority::waiver_phase_complete(contract, requested, evidence),
        diagnostics::preserved(old(unmet)@, final(unmet)@),
        crate::model::authority::invalid_waivers_reported(contract, requested, evidence, final(unmet)@),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let supplied_valid = supplied::evaluate(contract, requested, evidence, unmet);
    let ghost after_supplied = unmet@;
    let blockers_resolved = blockers::evaluate(contract, requested, evidence, unmet);
    proof { diagnostics::preserve_reports(contract, requested, evidence, after_supplied, unmet@, evidence.spec_waivers().len() as int); }
    supplied_valid && blockers_resolved
}

#[allow(
    clippy::option_if_let_else,
    reason = "explicit Option branches preserve the audited approval-diagnostic precedence in Verus"
)]
pub(super) fn evaluate_final_approval(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> (complete: bool)
    ensures
        complete == final_approval_complete(contract, requested, evidence),
        diagnostics::preserved(old(unmet)@, final(unmet)@),
        final(unmet)@ == match final_approval_failure(contract, requested, evidence) {
            None => old(unmet)@,
            Some(condition) => old(unmet)@.push(condition),
        },
{
    let failure = match contract.approval_policy() {
        HumanApprovalPolicy::NotRequired => {
            assert(final_approval_failure(contract, requested, evidence).is_none());
            None
        },
        HumanApprovalPolicy::Required(required_authority) => {
            match current_acceptance_approval(evidence, requested) {
                None => {
                    assert(!current_acceptance_approval_exists(
                        evidence.spec_approvals(), requested,
                    ));
                    assert(final_approval_failure(contract, requested, evidence)
                        == Some(UnmetCondition::MissingHumanApproval));
                    Some(UnmetCondition::MissingHumanApproval)
                },
                Some(index) => {
                    let approval = &evidence.approvals()[index];
                    proof {
                        assert(current_acceptance_approval_exists(
                            evidence.spec_approvals(), requested,
                        ));
                        assert(first_current_acceptance_has_authority(
                            evidence.spec_approvals(), requested, required_authority,
                        ) == crate::model::authority::authority_matches(
                            approval.spec_authority(), required_authority,
                        ));
                        assert(first_current_acceptance_approved(
                            evidence.spec_approvals(), requested,
                        ) == (approval.spec_outcome() == ApprovalOutcome::Approved));
                    }
                    if crate::revision::digest_matches(
                        approval.authority().digest(),
                        required_authority.digest(),
                    ) {
                        match approval.outcome() {
                            ApprovalOutcome::Denied => {
                                assert(final_approval_failure(contract, requested, evidence)
                                    == Some(UnmetCondition::HumanApprovalDenied));
                                Some(UnmetCondition::HumanApprovalDenied)
                            },
                            ApprovalOutcome::Approved => {
                                assert(final_approval_failure(contract, requested, evidence).is_none());
                                None
                            },
                        }
                    } else {
                        assert(final_approval_failure(contract, requested, evidence)
                            == Some(UnmetCondition::WrongHumanApprovalAuthority));
                        Some(UnmetCondition::WrongHumanApprovalAuthority)
                    }
                },
            }
        },
    };
    assert(failure == final_approval_failure(contract, requested, evidence));
    match failure {
        None => true,
        Some(condition) => {
            unmet.push(condition);
            false
        },
    }
}

fn current_acceptance_approval(
    evidence: &AcceptanceEvidence,
    requested: RevisionTuple,
) -> (result: Option<usize>)
    ensures match result {
        Some(index) => {
            &&& first_current_acceptance_approval_at(
                evidence.spec_approvals(), requested, index as int,
            )
            &&& forall |other: int| first_current_acceptance_approval_at(
                evidence.spec_approvals(), requested, other,
            ) ==> other == index
        },
        None => forall |index: int| !current_acceptance_approval_at(
            evidence.spec_approvals(), requested, index,
        ),
    },
{
    let mut index = 0;
    while index < evidence.approvals().len()
        invariant
            0 <= index <= evidence.spec_approvals().len(),
            forall |prior: int| 0 <= prior < index ==>
                !current_acceptance_approval_at(
                    evidence.spec_approvals(), requested, prior,
                ),
        decreases evidence.spec_approvals().len() - index,
    {
        let approval = &evidence.approvals()[index];
        if crate::revision::revision_matches(approval.revision(), requested)
            && lookup::subject_equal(approval.subject(), ApprovalSubject::Acceptance)
        {
            assert forall |other: int| first_current_acceptance_approval_at(
                evidence.spec_approvals(), requested, other,
            ) implies other == index by {
                if other < index {
                    assert(!current_acceptance_approval_at(
                        evidence.spec_approvals(), requested, other,
                    ));
                } else if other > index {
                    assert(!first_current_acceptance_approval_at(
                        evidence.spec_approvals(), requested, other,
                    )) by {
                        assert(current_acceptance_approval_at(
                            evidence.spec_approvals(), requested, index as int,
                        ));
                    };
                }
            }
            return Some(index);
        }
        index += 1;
    }
    None
}

fn waiver_approval_expected(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: &crate::ApprovalObservation,
    finding: peritus_types::FindingId,
) -> (expected: bool)
    ensures expected == crate::model::approvals::waiver_approval_expected(
        contract, requested, evidence, *approval, finding,
    ),
{
    let mut found = false;
    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len()
        invariant
            0 <= waiver_index <= evidence.spec_waivers().len(),
            found == (exists |waiver: int|
                waiver < waiver_index
                && #[trigger] waiver_approval_witness(
                    contract, requested, evidence, *approval, finding, waiver,
                )),
        decreases evidence.spec_waivers().len() - waiver_index,
    {
        let ghost found_before = found;
        let waiver = &evidence.waivers()[waiver_index];
        if crate::revision::revision_matches(waiver.revision(), requested)
            && lookup::finding_equal(waiver.finding_id(), finding)
            && lookup::request_equal(waiver.approval_request_id(), approval.request_id())
            && waiver::supplied_failure(contract, requested, evidence, waiver).is_none()
        {
            assert(waiver_approval_witness(
                contract, requested, evidence, *approval, finding, waiver_index as int,
            ));
            found = true;
        }
        if !found {
            assert(!waiver_approval_witness(
                contract, requested, evidence, *approval, finding, waiver_index as int,
            ));
        }
        assert(found == (exists |candidate: int|
            candidate < waiver_index + 1
            && #[trigger] waiver_approval_witness(
                contract, requested, evidence, *approval, finding, candidate,
            ))) by {
            if found_before {
                let candidate = choose |candidate: int|
                    candidate < waiver_index
                    && waiver_approval_witness(
                        contract, requested, evidence, *approval, finding, candidate,
                    );
                assert(candidate < waiver_index + 1);
            } else if found {
                assert(waiver_approval_witness(
                    contract, requested, evidence, *approval, finding, waiver_index as int,
                ));
            } else {
                assert forall |candidate: int| candidate < waiver_index + 1 implies
                    !waiver_approval_witness(
                        contract, requested, evidence, *approval, finding, candidate,
                    ) by {
                    if candidate < waiver_index {
                        assert(!waiver_approval_witness(
                            contract, requested, evidence, *approval, finding, candidate,
                        ));
                    } else {
                        assert(candidate == waiver_index);
                    }
                }
            }
        }
        waiver_index += 1;
    }
    found
}

fn approval_expected(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: &crate::ApprovalObservation,
) -> (expected: bool)
    ensures expected == crate::model::approvals::approval_expected(
        contract, requested, evidence, *approval,
    ),
{
    match approval.subject() {
        ApprovalSubject::Acceptance => contract.approval_policy().is_required(),
        ApprovalSubject::FindingWaiver(finding) => {
            waiver_approval_expected(contract, requested, evidence, approval, finding)
        },
    }
}

pub(super) fn evaluate_unexpected_approvals(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> (complete: bool)
    ensures
        complete == unexpected_approvals_complete(contract, requested, evidence),
        diagnostics::preserved(old(unmet)@, final(unmet)@),
        complete ==> final(unmet)@ == old(unmet)@,
        !complete ==> old(unmet)@.len() < final(unmet)@.len(),
{
    let mut complete = true;
    let mut index = 0;
    while index < evidence.approvals().len()
        invariant
            0 <= index <= evidence.spec_approvals().len(),
            diagnostics::preserved(old(unmet)@, unmet@),
            complete == (forall |prior: int| 0 <= prior < index
                && crate::model::revision_fresh(
                    #[trigger] evidence.spec_approvals()[prior].spec_revision(), requested,
                ) ==> crate::model::approvals::approval_expected(
                    contract, requested, evidence, evidence.spec_approvals()[prior],
                )),
            complete ==> unmet@ == old(unmet)@,
            !complete ==> old(unmet)@.len() < unmet@.len(),
        decreases evidence.spec_approvals().len() - index,
    {
        let approval = &evidence.approvals()[index];
        if crate::revision::revision_matches(approval.revision(), requested)
            && !approval_expected(contract, requested, evidence, approval)
        {
            complete = false;
            unmet.push(UnmetCondition::UnexpectedApproval(approval.actor_id()));
        }
        index += 1;
    }
    complete
}

} // verus!
