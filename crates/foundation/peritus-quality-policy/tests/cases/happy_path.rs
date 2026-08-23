use crate::support::{ContractOptions, Fixture};
use peritus_quality_policy::{ApprovalOutcome, evaluate_acceptance};
use peritus_spec::{FindingSeverity, HumanApprovalPolicy, ReviewerIndependence, WaiverPolicy};

#[test]
fn complete_exact_revision_evidence_is_acceptable() {
    let fixture = Fixture::new();
    let mut options = ContractOptions::basic();
    options.quorum = 2;
    options.independence = ReviewerIndependence::new(true, true, true, true, true, true);
    options.blocking_severity = FindingSeverity::High;
    options.approval_policy = HumanApprovalPolicy::Required(fixture.approval_authority);
    options.waiver_policy = WaiverPolicy::Allowed {
        authority: fixture.waiver_authority,
        evidence: fixture.waiver_evidence,
    };
    let contract = fixture.contract(options);
    let revision = fixture.revision();
    let reviews = vec![
        fixture.review(
            revision,
            70,
            80,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            130,
            true,
        ),
        fixture.review(
            revision,
            71,
            81,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            140,
            true,
        ),
    ];
    let evidence = fixture.evidence_set(
        &contract,
        revision,
        reviews,
        vec![fixture.acceptance_approval(revision, ApprovalOutcome::Approved)],
        Vec::new(),
    );

    let decision = evaluate_acceptance(&contract, revision, &evidence);

    assert!(decision.is_acceptable(), "{:?}", decision.unmet_conditions());
    assert!(decision.unmet_conditions().is_empty());
}
