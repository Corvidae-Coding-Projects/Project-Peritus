//! Canonical D2 semantic digest fixtures.

#![allow(clippy::unwrap_used, reason = "fixed codec fixture uses checked values")]

use peritus_review::{Confidence, Finding, FindingLocation, FindingSource, ReviewLimits};
use peritus_spec::{FindingSeverity, RequirementId, ReviewCategory};
use peritus_types::{
    AcceptanceSpecId, ActorId, EvidenceId, FindingId, Generation, HarnessId, PolicyId,
    ProviderProfileId, ReviewCycleId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

#[test]
fn semantic_finding_digest_ignores_identity_provenance_and_confidence() {
    let first = finding(1, 2, 7_000, 3, "same defect");
    let second = finding(4, 5, 9_000, 6, "same defect");
    assert_eq!(first.normalized_digest(), second.normalized_digest());

    let changed = finding(7, 8, 7_000, 9, "different defect");
    assert_ne!(first.normalized_digest(), changed.normalized_digest());
}

fn finding(id: u8, cycle: u8, confidence: u16, evidence: u8, description: &str) -> Finding {
    let limits = limits();
    Finding::new(
        FindingId::new(bytes(id)).unwrap(),
        FindingSource::new(
            ReviewCycleId::new(bytes(cycle)).unwrap(),
            ActorId::new(bytes(cycle.wrapping_add(20))).unwrap(),
        ),
        ReviewCategory::new(digest(30)),
        FindingSeverity::High,
        FindingSeverity::High,
        Confidence::new(confidence).unwrap(),
        vec![RequirementId::new(digest(31))],
        vec![FindingLocation::new("src/lib.rs".to_owned(), 1, 2, 3, 4, limits).unwrap()],
        vec![EvidenceId::new(bytes(evidence)).unwrap()],
        description.to_owned(),
        "reproduce".to_owned(),
        "expected".to_owned(),
        "remediate".to_owned(),
        revision(),
        limits,
    )
    .unwrap()
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(40)).unwrap(),
        HarnessId::new(bytes(41)).unwrap(),
        WorkspaceId::new(bytes(42)).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(43)).unwrap(),
        ProviderProfileId::new(bytes(44)).unwrap(),
    )
}

fn limits() -> ReviewLimits {
    ReviewLimits::new(
        16, 16, 16, 128, 16, 16, 16, 32, 16, 32, 256, 4_096, 4_096, 1_048_576, 4_194_304,
    )
    .unwrap()
}

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
