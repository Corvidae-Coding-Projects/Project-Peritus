//! Checked immutable evidence-domain construction tests.

mod support;

use peritus_artifact_store::ArtifactDigest;
use peritus_evidence::{
    CausalLink, EvidenceDraft, EvidenceErrorKind, EvidenceInvalidation, EvidenceKind,
    EvidenceSource,
};
use peritus_types::Sha256Digest;
use support::{event_id, evidence_id, revision};

#[test]
fn stable_tags_reject_noncanonical_spellings() {
    for value in ["", "Uppercase", "-leading", "trailing-", "two--dashes"] {
        assert_eq!(
            EvidenceKind::new(value).expect_err("invalid kind").kind(),
            EvidenceErrorKind::InvalidInput
        );
    }
    assert_eq!(EvidenceSource::new("local-runner").expect("valid source").as_str(), "local-runner");
}

#[test]
fn drafts_links_and_invalidations_enforce_structural_invariants() {
    let id = evidence_id(70);
    let self_caused = EvidenceDraft::new(
        id,
        EvidenceKind::new("execution-result").expect("kind"),
        EvidenceSource::new("local-runner").expect("source"),
        revision(),
        1,
        Sha256Digest::new([7; 32]),
        Vec::new(),
        vec![id],
    )
    .expect_err("self cause rejected");
    assert_eq!(self_caused.kind(), EvidenceErrorKind::InvalidInput);

    let high = ArtifactDigest::new([9; 32]);
    let low = ArtifactDigest::new([8; 32]);
    let unordered = EvidenceDraft::new(
        id,
        EvidenceKind::new("execution-result").expect("kind"),
        EvidenceSource::new("local-runner").expect("source"),
        revision(),
        1,
        Sha256Digest::new([7; 32]),
        vec![high, low],
        Vec::new(),
    )
    .expect_err("artifact order rejected");
    assert_eq!(unordered.kind(), EvidenceErrorKind::InvalidInput);
    assert_eq!(
        CausalLink::new(id, id).expect_err("reflexive link rejected").kind(),
        EvidenceErrorKind::InvalidCause
    );
    assert_eq!(
        EvidenceInvalidation::new(
            id,
            0,
            event_id(71),
            Sha256Digest::new([1; 32]),
            Sha256Digest::new([2; 32]),
        )
        .expect_err("zero invalidation position rejected")
        .kind(),
        EvidenceErrorKind::InvalidInput
    );
}
