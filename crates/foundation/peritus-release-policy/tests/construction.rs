//! Checked release-policy value construction contracts.

mod support;

use peritus_release_policy::{
    Architecture, CandidateId, ConstructionErrorKind, EvidenceBinding, EvidenceObservation,
    GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix, PrincipalId, ReleaseEvidence,
    ReviewId,
};

#[test]
fn zero_nominal_identities_are_rejected() {
    assert_eq!(
        CandidateId::new([0; 16]).expect_err("zero candidate id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        PrincipalId::new([0; 16]).expect_err("zero principal id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        ReviewId::new([0; 16]).expect_err("zero review id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        GitCommitId::sha1([0; 20]).expect_err("zero commit").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
}

#[test]
fn invalid_time_and_source_revision_bindings_are_rejected() {
    let candidate = support::candidate();
    assert_eq!(
        EvidenceBinding::new(candidate, 10, 9, 1, candidate.source_revision())
            .expect_err("inverted validity")
            .kind(),
        ConstructionErrorKind::InvalidValidityInterval
    );
    assert_eq!(
        EvidenceBinding::new(candidate, 10, 20, 0, candidate.source_revision())
            .expect_err("zero sequence")
            .kind(),
        ConstructionErrorKind::ZeroRevision
    );
}

#[test]
fn platform_slots_are_nominal_not_positional_guesswork() {
    let linux =
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::X86_64, support::digest(1))
            .expect("linux");
    let windows =
        PlatformIdentity::new(OperatingSystem::Windows, Architecture::X86_64, support::digest(2))
            .expect("windows");
    assert_eq!(
        PlatformMatrix::new(windows, linux, windows).expect_err("mislabeled slots").kind(),
        ConstructionErrorKind::InvalidPlatformMatrix
    );
}

#[test]
fn evidence_collection_bound_is_enforced() {
    let inputs = support::ready_inputs();
    let observation: EvidenceObservation = inputs.observations[0];
    let oversized = vec![observation; ReleaseEvidence::MAX_COLLECTION_LEN + 1];
    assert_eq!(
        ReleaseEvidence::new(oversized, Vec::new(), Vec::new(), Vec::new(), Vec::new(),)
            .expect_err("oversized collection")
            .kind(),
        ConstructionErrorKind::CollectionLimitExceeded
    );
}
