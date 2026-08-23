use crate::support::{ContractOptions, Fixture, bytes, digest, finding_id};
use peritus_quality_policy::{
    ApprovalObservation, ApprovalOutcome, ApprovalSubject, FindingDisposition, FindingObservation,
    InvalidWaiverReason, UnmetCondition, WaiverObservation, evaluate_acceptance,
};
use peritus_spec::{FindingSeverity, HumanApprovalPolicy, WaiverPolicy};
use peritus_types::{ActorId, ApprovalRequestId};

fn low_severity_waiver_evidence(
    fixture: &Fixture,
    contract: &peritus_spec::AcceptanceContract,
    authority: peritus_spec::ContentReference,
    outcome: ApprovalOutcome,
) -> (peritus_quality_policy::AcceptanceEvidence, peritus_types::FindingId, ActorId) {
    let revision = fixture.revision();
    let finding_id = finding_id(50);
    let request_id = ApprovalRequestId::new(bytes(90)).expect("approval request");
    let actor_id = ActorId::new(bytes(91)).expect("human actor");
    let review = fixture.review(
        revision,
        70,
        80,
        vec![fixture.category_a, fixture.category_b],
        vec![FindingObservation::new(
            finding_id,
            FindingSeverity::Low,
            FindingDisposition::WaiverRequested,
            digest(51),
        )],
        130,
        true,
    );
    let approval = ApprovalObservation::new(
        request_id,
        revision,
        ApprovalSubject::FindingWaiver(finding_id),
        actor_id,
        authority,
        outcome,
        digest(92),
    );
    let waiver = WaiverObservation::new(
        finding_id,
        revision,
        request_id,
        authority,
        fixture.waiver_evidence,
        digest(93),
    );
    (
        fixture.evidence_set(contract, revision, vec![review], vec![approval], vec![waiver]),
        finding_id,
        actor_id,
    )
}

#[test]
fn required_human_approval_must_be_current_approved_and_from_declared_authority() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.approval_policy = HumanApprovalPolicy::Required(fixture.approval_authority);
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let review = || {
        fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        )
    };

    let missing = fixture.evidence_set(&contract, revision, vec![review()], Vec::new(), Vec::new());
    assert!(
        evaluate_acceptance(&contract, revision, &missing)
            .unmet_conditions()
            .contains(&UnmetCondition::MissingHumanApproval)
    );

    let denied = fixture.evidence_set(
        &contract,
        revision,
        vec![review()],
        vec![fixture.acceptance_approval(revision, ApprovalOutcome::Denied)],
        Vec::new(),
    );
    assert!(
        evaluate_acceptance(&contract, revision, &denied)
            .unmet_conditions()
            .contains(&UnmetCondition::HumanApprovalDenied)
    );

    let wrong = ApprovalObservation::new(
        ApprovalRequestId::new(bytes(90)).expect("approval"),
        revision,
        ApprovalSubject::Acceptance,
        ActorId::new(bytes(91)).expect("actor"),
        crate::support::content(112),
        ApprovalOutcome::Approved,
        digest(92),
    );
    let wrong_authority =
        fixture.evidence_set(&contract, revision, vec![review()], vec![wrong], Vec::new());
    assert!(
        evaluate_acceptance(&contract, revision, &wrong_authority)
            .unmet_conditions()
            .contains(&UnmetCondition::WrongHumanApprovalAuthority)
    );
}

#[test]
fn explicit_valid_waiver_resolves_a_requested_blocker() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.waiver_policy = WaiverPolicy::Allowed {
        authority: fixture.waiver_authority,
        evidence: fixture.waiver_evidence,
    };
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let finding_id = finding_id(50);
    let request_id = ApprovalRequestId::new(bytes(90)).expect("approval request");
    let review = fixture.review(
        revision,
        70,
        80,
        vec![fixture.category_a, fixture.category_b],
        vec![FindingObservation::new(
            finding_id,
            FindingSeverity::Critical,
            FindingDisposition::WaiverRequested,
            digest(51),
        )],
        130,
        true,
    );
    let approval = ApprovalObservation::new(
        request_id,
        revision,
        ApprovalSubject::FindingWaiver(finding_id),
        ActorId::new(bytes(91)).expect("human actor"),
        fixture.waiver_authority,
        ApprovalOutcome::Approved,
        digest(92),
    );
    let waiver = WaiverObservation::new(
        finding_id,
        revision,
        request_id,
        fixture.waiver_authority,
        fixture.waiver_evidence,
        digest(93),
    );
    let evidence =
        fixture.evidence_set(&contract, revision, vec![review], vec![approval], vec![waiver]);
    let decision = evaluate_acceptance(&contract, revision, &evidence);
    assert!(decision.is_acceptable(), "{:?}", decision.unmet_conditions());
}

#[test]
fn forbidden_or_unapproved_waiver_never_resolves_a_blocker() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let revision = fixture.revision();
    let finding_id = finding_id(50);
    let request_id = ApprovalRequestId::new(bytes(90)).expect("approval request");
    let review = fixture.review(
        revision,
        70,
        80,
        vec![fixture.category_a, fixture.category_b],
        vec![FindingObservation::new(
            finding_id,
            FindingSeverity::High,
            FindingDisposition::WaiverRequested,
            digest(51),
        )],
        130,
        true,
    );
    let waiver = WaiverObservation::new(
        finding_id,
        revision,
        request_id,
        fixture.waiver_authority,
        fixture.waiver_evidence,
        digest(93),
    );
    let evidence =
        fixture.evidence_set(&contract, revision, vec![review], Vec::new(), vec![waiver]);
    let decision = evaluate_acceptance(&contract, revision, &evidence);
    assert!(decision.unmet_conditions().contains(&UnmetCondition::InvalidWaiver {
        finding_id,
        reason: InvalidWaiverReason::Forbidden,
    }));
    assert!(decision.unmet_conditions().iter().any(|condition| matches!(
        condition,
        UnmetCondition::UnwaivedBlocker { finding_id: actual, .. } if *actual == finding_id
    )));
}

#[test]
fn forbidden_waiver_for_non_blocking_finding_is_rejected_once() {
    let fixture = Fixture::new();
    let contract = fixture.contract(ContractOptions::basic());
    let (evidence, finding_id, actor_id) = low_severity_waiver_evidence(
        &fixture,
        &contract,
        fixture.waiver_authority,
        ApprovalOutcome::Approved,
    );

    assert_eq!(
        evaluate_acceptance(&contract, fixture.revision(), &evidence).unmet_conditions(),
        &[
            UnmetCondition::InvalidWaiver { finding_id, reason: InvalidWaiverReason::Forbidden },
            UnmetCondition::UnexpectedApproval(actor_id),
        ]
    );
}

#[test]
fn denied_waiver_for_non_blocking_finding_is_rejected_once() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.waiver_policy = WaiverPolicy::Allowed {
        authority: fixture.waiver_authority,
        evidence: fixture.waiver_evidence,
    };
    let contract = fixture.contract(options);
    let (evidence, finding_id, actor_id) = low_severity_waiver_evidence(
        &fixture,
        &contract,
        fixture.waiver_authority,
        ApprovalOutcome::Denied,
    );

    assert_eq!(
        evaluate_acceptance(&contract, fixture.revision(), &evidence).unmet_conditions(),
        &[
            UnmetCondition::InvalidWaiver {
                finding_id,
                reason: InvalidWaiverReason::ApprovalDenied,
            },
            UnmetCondition::UnexpectedApproval(actor_id),
        ]
    );
}

#[test]
fn wrong_authority_waiver_for_non_blocking_finding_is_rejected_once() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.waiver_policy = WaiverPolicy::Allowed {
        authority: fixture.waiver_authority,
        evidence: fixture.waiver_evidence,
    };
    let contract = fixture.contract(options);
    let wrong_authority = crate::support::content(112);
    let (evidence, finding_id, actor_id) = low_severity_waiver_evidence(
        &fixture,
        &contract,
        wrong_authority,
        ApprovalOutcome::Approved,
    );

    assert_eq!(
        evaluate_acceptance(&contract, fixture.revision(), &evidence).unmet_conditions(),
        &[
            UnmetCondition::InvalidWaiver {
                finding_id,
                reason: InvalidWaiverReason::WrongAuthority,
            },
            UnmetCondition::UnexpectedApproval(actor_id),
        ]
    );
}
