//! Acceptance component, evidence, and authority-policy validation tests.

mod support;

use peritus_spec::{
    AcceptanceContract, Assumption, CanonicalCollection, CompletionPolicy, EvidenceSource,
    Exclusion, FindingSeverity, HumanApprovalPolicy, LimitKind, Requirement, ReviewPolicy,
    ReviewerIndependence, SpecError, WaiverPolicy,
};

#[test]
fn review_and_completion_policies_reject_zero_or_ambiguous_inputs() {
    let independence = ReviewerIndependence::new(true, true, true, false, false, true);
    assert_eq!(
        ReviewPolicy::new(vec![], 1, independence, FindingSeverity::High),
        Err(SpecError::EmptyCollection(CanonicalCollection::ReviewCategories))
    );
    assert_eq!(
        ReviewPolicy::new(
            vec![support::category(1), support::category(1)],
            1,
            independence,
            FindingSeverity::High,
        ),
        Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::ReviewCategories))
    );
    assert_eq!(
        ReviewPolicy::new(vec![support::category(1)], 0, independence, FindingSeverity::High),
        Err(SpecError::ZeroLimit(LimitKind::ReviewerQuorum))
    );
    assert_eq!(CompletionPolicy::new(0, 1), Err(SpecError::ZeroLimit(LimitKind::GateAttempts)));
    assert_eq!(CompletionPolicy::new(1, 0), Err(SpecError::ZeroLimit(LimitKind::ReviewCycles)));
}

#[test]
fn contract_rejects_quorum_that_cannot_fit_in_review_cycles() {
    let review = ReviewPolicy::new(
        vec![support::category(1)],
        3,
        ReviewerIndependence::new(true, true, true, false, false, true),
        FindingSeverity::High,
    )
    .expect("review policy");
    let result = AcceptanceContract::new(
        support::acceptance_id(1),
        support::digest(90),
        support::documents(),
        vec![Requirement::new(support::requirement_id(1), support::content(1))],
        vec![],
        vec![],
        support::graph(),
        review,
        vec![
            support::evidence(1, EvidenceSource::Gate(support::gate_id(1))),
            support::evidence(2, EvidenceSource::Gate(support::gate_id(2))),
        ],
        CompletionPolicy::new(1, 2).expect("limits"),
        HumanApprovalPolicy::NotRequired,
        WaiverPolicy::Forbidden,
    );

    assert_eq!(
        result,
        Err(SpecError::ReviewQuorumExceedsCycleLimit { reviewer_quorum: 3, max_review_cycles: 2 })
    );
}

#[test]
fn contract_preserves_checked_components() {
    let contract = support::contract();
    assert_eq!(contract.requirements()[0].id(), support::requirement_id(1));
    assert_eq!(contract.gates().definitions().len(), 2);
    assert_eq!(contract.review_policy().reviewer_quorum(), 2);
    assert_eq!(contract.evidence_requirements().len(), 6);
    assert!(contract.approval_policy().is_required());
    assert!(contract.waiver_policy().is_allowed());
    assert_eq!(contract.completion_policy().max_review_cycles(), 4);
}

#[test]
fn contract_rejects_duplicate_requirements_exclusions_and_assumptions() {
    let build = |requirements, exclusions, assumptions| {
        AcceptanceContract::new(
            support::acceptance_id(1),
            support::digest(90),
            support::documents(),
            requirements,
            exclusions,
            assumptions,
            support::graph(),
            support::review_policy(),
            vec![
                support::evidence(1, EvidenceSource::Gate(support::gate_id(1))),
                support::evidence(2, EvidenceSource::Gate(support::gate_id(2))),
            ],
            CompletionPolicy::new(1, 2).expect("limits"),
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Forbidden,
        )
    };

    assert!(matches!(
        build(vec![], vec![], vec![]),
        Err(SpecError::EmptyCollection(CanonicalCollection::Requirements))
    ));
    assert!(matches!(
        build(
            vec![
                Requirement::new(support::requirement_id(1), support::content(1)),
                Requirement::new(support::requirement_id(1), support::content(2)),
            ],
            vec![],
            vec![],
        ),
        Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Requirements))
    ));
    assert!(matches!(
        build(
            vec![Requirement::new(support::requirement_id(1), support::content(1))],
            vec![Exclusion::new(support::content(2)), Exclusion::new(support::content(2))],
            vec![],
        ),
        Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Exclusions))
    ));
    assert!(matches!(
        build(
            vec![Requirement::new(support::requirement_id(1), support::content(1))],
            vec![],
            vec![Assumption::new(support::content(2)), Assumption::new(support::content(2))],
        ),
        Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Assumptions))
    ));
}

#[test]
fn contract_rejects_dangling_and_mistyped_evidence() {
    let base = |evidence, approval, waiver| {
        AcceptanceContract::new(
            support::acceptance_id(1),
            support::digest(90),
            support::documents(),
            vec![Requirement::new(support::requirement_id(1), support::content(1))],
            vec![],
            vec![],
            support::graph(),
            support::review_policy(),
            evidence,
            CompletionPolicy::new(1, 2).expect("limits"),
            approval,
            waiver,
        )
    };

    assert!(matches!(
        base(
            vec![
                support::evidence(1, EvidenceSource::Gate(support::gate_id(1))),
                support::evidence(2, EvidenceSource::Gate(support::gate_id(9))),
            ],
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Forbidden,
        ),
        Err(SpecError::InvalidEvidenceSource(_))
    ));
    assert!(matches!(
        base(
            vec![
                support::evidence(1, EvidenceSource::Gate(support::gate_id(2))),
                support::evidence(2, EvidenceSource::Gate(support::gate_id(2))),
            ],
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Forbidden,
        ),
        Err(SpecError::InvalidEvidenceSource(id)) if id == support::evidence_id(1)
    ));
    assert!(matches!(
        base(
            vec![
                support::evidence(1, EvidenceSource::Gate(support::gate_id(1))),
                support::evidence(2, EvidenceSource::Gate(support::gate_id(2))),
            ],
            HumanApprovalPolicy::Required(support::content(8)),
            WaiverPolicy::Forbidden,
        ),
        Err(SpecError::MissingApprovalEvidence)
    ));
    assert_eq!(
        base(
            vec![
                support::evidence(1, EvidenceSource::Gate(support::gate_id(1))),
                support::evidence(2, EvidenceSource::Gate(support::gate_id(2))),
                support::evidence(3, EvidenceSource::General),
            ],
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Allowed {
                authority: support::content(8),
                evidence: support::evidence_id(3),
            },
        ),
        Err(SpecError::InvalidWaiverEvidence)
    );
}
