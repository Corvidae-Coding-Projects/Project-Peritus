use crate::support::{ContractOptions, Fixture, bytes, digest, finding_id};
use peritus_quality_policy::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject, FindingDisposition,
    FindingObservation, InvalidWaiverReason, UnmetCondition, WaiverObservation,
    evaluate_acceptance,
};
use peritus_spec::{
    AcceptanceContract, ContentReference, EvidenceRequirementId, FindingSeverity,
    HumanApprovalPolicy, WaiverPolicy,
};
use peritus_types::{ActorId, ApprovalRequestId, FindingId, Sha256Digest};

#[derive(Clone, Copy)]
enum Rejection {
    UnknownFinding,
    Resolved,
    Open,
    Forbidden,
    WaiverAuthority,
    Requirement,
    Artifact,
    ApprovalRequest,
    ApprovalRevision,
    ApprovalAuthority,
    Denied,
}

const fn changed_digest(value: Sha256Digest) -> Sha256Digest {
    let mut value = value.into_bytes();
    value[31] ^= 1;
    Sha256Digest::new(value)
}

fn review_finding(fixture: &Fixture, scenario: Rejection) -> FindingObservation {
    let disposition = match scenario {
        Rejection::Resolved => FindingDisposition::Resolved {
            revision: fixture.revision(),
            evidence_digest: digest(53),
        },
        Rejection::Open => FindingDisposition::Open,
        _ => FindingDisposition::WaiverRequested,
    };
    FindingObservation::new(finding_id(50), FindingSeverity::Low, disposition, digest(51))
}

fn approval(fixture: &Fixture, scenario: Rejection, finding: FindingId) -> ApprovalObservation {
    let mut request = bytes(90);
    if matches!(scenario, Rejection::ApprovalRequest) {
        request[15] ^= 1;
    }
    let authority = if matches!(scenario, Rejection::ApprovalAuthority) {
        ContentReference::new(changed_digest(fixture.waiver_authority.digest()))
    } else {
        fixture.waiver_authority
    };
    let outcome = if matches!(scenario, Rejection::Denied | Rejection::ApprovalAuthority) {
        ApprovalOutcome::Denied
    } else {
        ApprovalOutcome::Approved
    };
    let revision = if matches!(scenario, Rejection::ApprovalRevision) {
        fixture.revision_from([1, 2, 3, 1, 2, 4, 5])
    } else {
        fixture.revision()
    };
    ApprovalObservation::new(
        ApprovalRequestId::new(request).expect("request"),
        revision,
        ApprovalSubject::FindingWaiver(finding),
        ActorId::new(bytes(91)).expect("actor"),
        authority,
        outcome,
        digest(92),
    )
}

fn rejected_evidence(
    fixture: &Fixture,
    scenario: Rejection,
) -> (AcceptanceContract, AcceptanceEvidence, FindingId) {
    let mut options = ContractOptions::basic();
    options.approval_policy = HumanApprovalPolicy::Required(fixture.approval_authority);
    if !matches!(scenario, Rejection::Forbidden | Rejection::Resolved | Rejection::Open) {
        options.waiver_policy = WaiverPolicy::Allowed {
            authority: fixture.waiver_authority,
            evidence: fixture.waiver_evidence,
        };
    }
    let contract = fixture.contract(options);
    let review = fixture.review(
        fixture.revision(),
        70,
        80,
        vec![fixture.category_a, fixture.category_b],
        vec![review_finding(fixture, scenario)],
        130,
        true,
    );
    let mut waiver_finding = bytes(50);
    if matches!(scenario, Rejection::UnknownFinding) {
        waiver_finding[15] ^= 1;
    }
    let waiver_finding = FindingId::new(waiver_finding).expect("finding");
    let authority = if matches!(scenario, Rejection::WaiverAuthority | Rejection::Forbidden) {
        ContentReference::new(changed_digest(fixture.waiver_authority.digest()))
    } else {
        fixture.waiver_authority
    };
    let requirement = if matches!(scenario, Rejection::Requirement | Rejection::WaiverAuthority) {
        EvidenceRequirementId::new(changed_digest(fixture.waiver_evidence.digest()))
    } else {
        fixture.waiver_evidence
    };
    let waiver = WaiverObservation::new(
        waiver_finding,
        fixture.revision(),
        ApprovalRequestId::new(bytes(90)).expect("request"),
        authority,
        requirement,
        digest(93),
    );
    let mut artifacts = fixture.required_evidence(&contract, fixture.revision());
    if matches!(scenario, Rejection::Artifact | Rejection::Requirement) {
        artifacts.retain(|value| value.requirement_id() != fixture.waiver_evidence);
    }
    let approvals = if matches!(scenario, Rejection::Artifact) {
        Vec::new()
    } else {
        vec![approval(fixture, scenario, waiver_finding)]
    };
    let evidence = AcceptanceEvidence::new(
        vec![fixture.gate(fixture.revision(), peritus_quality_policy::GateOutcome::Passed)],
        vec![review],
        artifacts,
        approvals,
        vec![waiver],
    )
    .expect("canonical evidence");
    (contract, evidence, waiver_finding)
}

#[test]
fn invalid_waiver_priority_survives_later_approval_diagnostics() {
    let fixture = Fixture::new();
    for (scenario, expected) in [
        (Rejection::UnknownFinding, InvalidWaiverReason::UnknownFinding),
        (Rejection::Resolved, InvalidWaiverReason::AlreadyResolved),
        (Rejection::Open, InvalidWaiverReason::NotRequested),
        (Rejection::Forbidden, InvalidWaiverReason::Forbidden),
        (Rejection::WaiverAuthority, InvalidWaiverReason::WrongAuthority),
        (Rejection::Requirement, InvalidWaiverReason::WrongEvidenceRequirement),
        (Rejection::Artifact, InvalidWaiverReason::MissingEvidence),
        (Rejection::ApprovalRequest, InvalidWaiverReason::MissingApproval),
        (Rejection::ApprovalRevision, InvalidWaiverReason::MissingApproval),
        (Rejection::ApprovalAuthority, InvalidWaiverReason::MissingApproval),
        (Rejection::Denied, InvalidWaiverReason::ApprovalDenied),
    ] {
        let (contract, evidence, finding_id) = rejected_evidence(&fixture, scenario);
        let decision = evaluate_acceptance(&contract, fixture.revision(), &evidence);
        assert!(!decision.is_acceptable());
        let conditions = decision.unmet_conditions();
        let invalid: Vec<_> = conditions
            .iter()
            .filter(|condition| matches!(condition, UnmetCondition::InvalidWaiver { .. }))
            .collect();
        assert_eq!(invalid, [&UnmetCondition::InvalidWaiver { finding_id, reason: expected }]);
        let waiver_position = conditions
            .iter()
            .position(|condition| matches!(condition, UnmetCondition::InvalidWaiver { .. }))
            .expect("waiver diagnostic");
        let final_position = conditions
            .iter()
            .position(|condition| *condition == UnmetCondition::MissingHumanApproval)
            .expect("later final approval diagnostic");
        assert!(waiver_position < final_position);
        assert!(
            !conditions
                .iter()
                .any(|condition| matches!(condition, UnmetCondition::UnwaivedBlocker { .. }))
        );
    }
}

#[test]
fn every_severity_threshold_preserves_open_and_freshly_resolved_behavior() {
    let fixture = Fixture::new();
    let severities = [
        FindingSeverity::Advisory,
        FindingSeverity::Low,
        FindingSeverity::Medium,
        FindingSeverity::High,
        FindingSeverity::Critical,
    ];
    for (threshold_index, threshold) in severities.iter().enumerate() {
        let mut options = ContractOptions::basic();
        options.blocking_severity = *threshold;
        let contract = fixture.contract(options);
        for (severity_index, severity) in severities.iter().enumerate() {
            for resolved in [false, true] {
                let disposition = if resolved {
                    FindingDisposition::Resolved {
                        revision: fixture.revision(),
                        evidence_digest: digest(53),
                    }
                } else {
                    FindingDisposition::Open
                };
                let review = fixture.review(
                    fixture.revision(),
                    70,
                    80,
                    vec![fixture.category_a, fixture.category_b],
                    vec![FindingObservation::new(
                        finding_id(50),
                        *severity,
                        disposition,
                        digest(51),
                    )],
                    130,
                    true,
                );
                let evidence = fixture.evidence_set(
                    &contract,
                    fixture.revision(),
                    vec![review],
                    Vec::new(),
                    Vec::new(),
                );
                let decision = evaluate_acceptance(&contract, fixture.revision(), &evidence);
                assert_eq!(decision.is_acceptable(), resolved || severity_index < threshold_index);
            }
        }
    }
}
