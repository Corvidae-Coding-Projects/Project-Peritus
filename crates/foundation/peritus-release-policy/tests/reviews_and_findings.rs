//! Independent review, finding, and waiver contracts.

mod support;

use peritus_release_policy::{
    Diagnostic, FindingDisposition, FindingId, FindingObservation, FindingSeverity, ReleaseVerdict,
    ReviewId, ReviewObservation, ReviewOutcome, WaiverObservation,
};
use support::{binding, digest, principal, ready_inputs};

#[test]
fn independent_review_quorum_is_mandatory() {
    let mut inputs = ready_inputs();
    inputs.reviews.pop();
    let decision = inputs.evaluate();
    assert_eq!(decision.verdict(), ReleaseVerdict::NotReadyForProduction);
    assert!(
        decision.diagnostics().contains(&Diagnostic::ReviewerQuorum { required: 2, observed: 1 })
    );
}

#[test]
fn self_review_changes_required_and_shared_context_each_block() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    let producer = principal(80);
    inputs.reviews = vec![
        ReviewObservation::new(
            ReviewId::new([10; 16]).expect("review id"),
            binding(&candidate, 700),
            producer,
            producer,
            digest(230),
            digest(231),
            ReviewOutcome::Approved,
            false,
        )
        .expect("self review"),
        ReviewObservation::new(
            ReviewId::new([11; 16]).expect("review id"),
            binding(&candidate, 701),
            principal(81),
            producer,
            digest(230),
            digest(232),
            ReviewOutcome::ChangesRequired,
            true,
        )
        .expect("changes-required review"),
    ];
    let decision = inputs.evaluate();
    assert!(decision.diagnostics().contains(&Diagnostic::SelfReview(1)));
    assert!(decision.diagnostics().contains(&Diagnostic::NonIndependentReview(1)));
    assert!(decision.diagnostics().contains(&Diagnostic::ChangesRequired(1)));
    assert!(decision.diagnostics().contains(&Diagnostic::SharedReviewContext));
}

#[test]
fn unresolved_release_blocker_cannot_be_waived() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    let finding_id = FindingId::new([20; 16]).expect("finding id");
    let reporter = principal(82);
    inputs.findings.push(
        FindingObservation::new(
            finding_id,
            binding(&candidate, 710),
            reporter,
            FindingSeverity::Critical,
            true,
            FindingDisposition::WaiverRequested,
            digest(233),
        )
        .expect("release blocker"),
    );
    inputs.waivers.push(
        WaiverObservation::new(
            finding_id,
            binding(&candidate, 711),
            principal(83),
            digest(234),
            digest(235),
            true,
        )
        .expect("attempted waiver"),
    );
    let decision = inputs.evaluate();
    assert!(decision.diagnostics().contains(&Diagnostic::ReleaseBlockingFindings(1)));
    assert!(decision.diagnostics().contains(&Diagnostic::OpenFindings(1)));
    assert!(decision.diagnostics().contains(&Diagnostic::InvalidWaivers(1)));
}

#[test]
fn independent_waiver_can_resolve_a_nonblocking_finding() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    let finding_id = FindingId::new([21; 16]).expect("finding id");
    let reporter = principal(84);
    inputs.findings.push(
        FindingObservation::new(
            finding_id,
            binding(&candidate, 720),
            reporter,
            FindingSeverity::Medium,
            false,
            FindingDisposition::WaiverRequested,
            digest(236),
        )
        .expect("nonblocking finding"),
    );
    inputs.waivers.push(
        WaiverObservation::new(
            finding_id,
            binding(&candidate, 721),
            principal(85),
            digest(237),
            digest(238),
            true,
        )
        .expect("valid waiver"),
    );
    let decision = inputs.evaluate();
    assert_eq!(decision.verdict(), ReleaseVerdict::Ready);
    assert!(decision.findings().is_satisfied());
}

#[test]
fn ignored_and_quarantined_findings_are_never_release_ready() {
    for (seed, disposition, diagnostic) in [
        (22, FindingDisposition::Ignored, Diagnostic::IgnoredFindings(1)),
        (23, FindingDisposition::Quarantined, Diagnostic::QuarantinedFindings(1)),
    ] {
        let mut inputs = ready_inputs();
        let candidate = inputs.candidate;
        inputs.findings.push(
            FindingObservation::new(
                FindingId::new([seed; 16]).expect("finding id"),
                binding(&candidate, 730 + u64::from(seed)),
                principal(86),
                FindingSeverity::Low,
                false,
                disposition,
                digest(seed.wrapping_add(39)),
            )
            .expect("finding"),
        );
        assert!(inputs.evaluate().diagnostics().contains(&diagnostic));
    }
}
