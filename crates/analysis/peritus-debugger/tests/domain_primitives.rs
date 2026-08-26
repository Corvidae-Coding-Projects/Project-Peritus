//! Domain construction, containment, boundedness, and refinement coverage.

use std::collections::BTreeSet;

use peritus_artifact_store::ArtifactDigest;
use peritus_debugger::{
    ArtifactCitation, ConfidenceBasis, ConfidenceMillionths, DebuggerErrorKind, DebuggerJobId,
    DebuggerLimit, DebuggerLimits, DiagnosticText, FailureCategory, ReportClaim,
    UnsupportedConclusion, UnsupportedReason,
    verified::{
        BoundedAnalysisFacts, CitationContainmentFacts, NonAuthorityFacts, ReplayEquivalenceFacts,
        ReportValidityFacts, SelectionContainmentFacts, bounded_analysis, citation_containment,
        non_authority, replay_equivalence, report_validity, selection_containment,
    },
};
use peritus_types::Sha256Digest;

#[test]
fn nominal_identity_rejects_zero_and_derives_deterministically() {
    let error = DebuggerJobId::new([0; 16]).expect_err("zero must reject");
    assert_eq!(error.kind(), DebuggerErrorKind::InvalidInput);
    let left = DebuggerJobId::derive(b"peritus-test-job-v1\0", b"same")
        .expect("nonzero digest projection");
    let right = DebuggerJobId::derive(b"peritus-test-job-v1\0", b"same")
        .expect("nonzero digest projection");
    let other = DebuggerJobId::derive(b"peritus-test-job-v1\0", b"other")
        .expect("nonzero digest projection");
    assert_eq!(left, right);
    assert_ne!(left, other);
}

#[test]
fn limits_only_tighten_and_fit_c0_b3_boundaries() {
    let compiled = DebuggerLimits::production();
    assert_eq!(compiled.state_bytes(), 16 * 1024 * 1024);
    assert_eq!(compiled.event_bytes(), 16 * 1024 * 1024 - 16);
    let tightened =
        DebuggerLimits::tightened(&[(DebuggerLimit::Subjects, 2), (DebuggerLimit::Retries, 1)])
            .expect("canonical tightening");
    assert_eq!(tightened.get(DebuggerLimit::Subjects), 2);
    assert_eq!(tightened.retries(), 1);
    assert!(DebuggerLimits::tightened(&[(DebuggerLimit::Subjects, 0)]).is_err());
    assert!(
        DebuggerLimits::tightened(&[(DebuggerLimit::Retries, 1), (DebuggerLimit::Subjects, 1),])
            .is_err()
    );
    assert!(
        DebuggerLimits::tightened(&[(DebuggerLimit::StateBytes, compiled.state_bytes() + 1,)])
            .is_err()
    );
}

#[test]
fn taxonomy_is_complete_unique_and_strict() {
    assert_eq!(FailureCategory::ALL.len(), 49);
    let tags: BTreeSet<_> = FailureCategory::ALL.iter().map(|category| category.tag()).collect();
    assert_eq!(tags.len(), FailureCategory::ALL.len());
    for category in FailureCategory::ALL {
        assert_eq!(FailureCategory::from_tag(category.tag()).expect("known tag"), category);
    }
    assert!(FailureCategory::from_tag(0).is_err());
    assert!(FailureCategory::from_tag(u16::MAX).is_err());
}

#[test]
fn confidence_retains_counters_and_never_implies_acceptance() {
    let basis = ConfidenceBasis::new(3, 1, 2, 2, 1);
    let confidence = ConfidenceMillionths::calculate(basis).expect("support exists");
    assert_eq!(confidence.basis(), basis);
    assert!(confidence.value() < 1_000_000);
    assert!(ConfidenceMillionths::calculate(ConfidenceBasis::new(0, 0, 0, 0, 0)).is_err());
    assert!(
        ConfidenceMillionths::checked(1_000_000, ConfidenceBasis::new(1, 0, 1, 0, 0),).is_err()
    );
}

#[test]
fn unsupported_claim_retains_only_digest_and_reason() {
    let rejected =
        UnsupportedConclusion::new(Sha256Digest::new([7; 32]), UnsupportedReason::AuthorityClaim);
    let claim = ReportClaim::unsupported(rejected).expect("digest-only claim derives identity");
    assert_eq!(claim.unsupported_conclusion(), Some(rejected));
    assert!(claim.statement().is_none());
    assert!(claim.support().is_empty());
    assert_eq!(
        DiagnosticText::new("bounded diagnostic").expect("valid").as_str(),
        "bounded diagnostic"
    );
}

#[test]
fn artifact_ranges_are_nonempty_half_open() {
    let digest = ArtifactDigest::from_sha256(Sha256Digest::new([9; 32]));
    let citation = ArtifactCitation::new(digest, 2, 5).expect("valid range");
    assert_eq!(citation.start(), 2);
    assert_eq!(citation.end(), 5);
    assert!(ArtifactCitation::new(digest, 5, 5).is_err());
    assert!(ArtifactCitation::new(digest, 6, 5).is_err());
}

#[test]
fn refinement_selection_containment() {
    assert!(selection_containment(SelectionContainmentFacts::new(true, true, true, true, true,)));
    assert!(!selection_containment(SelectionContainmentFacts::new(true, true, false, true, true,)));
}

#[test]
fn refinement_citation_containment() {
    assert!(citation_containment(CitationContainmentFacts::new(
        true, true, true, true, true, true,
    )));
    assert!(!citation_containment(CitationContainmentFacts::new(
        true, true, true, true, true, false,
    )));
}

#[test]
fn refinement_report_validity() {
    assert!(report_validity(ReportValidityFacts::new(true, true, true, true, true, true,)));
    assert!(!report_validity(ReportValidityFacts::new(true, true, true, true, false, true,)));
}

#[test]
fn refinement_replay_equivalence() {
    assert!(replay_equivalence(ReplayEquivalenceFacts::new(true, true, true, true,)));
    assert!(!replay_equivalence(ReplayEquivalenceFacts::new(true, true, true, false,)));
}

#[test]
fn refinement_bounded_analysis() {
    assert!(bounded_analysis(BoundedAnalysisFacts::new(true, true, true, true, true, true,)));
    assert!(!bounded_analysis(BoundedAnalysisFacts::new(true, true, true, false, true, true,)));
}

#[test]
fn refinement_non_authority() {
    assert!(non_authority(NonAuthorityFacts::new(true, true, true, true, true, true,)));
    assert!(!non_authority(NonAuthorityFacts::new(true, true, true, false, true, true,)));
}
