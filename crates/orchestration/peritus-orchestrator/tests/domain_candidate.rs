//! Exact candidate material and clean-quality snapshot binding behavior.

use peritus_orchestrator::{CandidateBinding, OrchestratorErrorKind, OrchestratorLimits};
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, SnapshotId, WorkspaceId,
};

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(1)).expect("acceptance id is nonzero"),
        HarnessId::new(bytes(2)).expect("harness id is nonzero"),
        WorkspaceId::new(bytes(3)).expect("workspace id is nonzero"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(4)).expect("policy id is nonzero"),
        ProviderProfileId::new(bytes(5)).expect("provider id is nonzero"),
    )
}

fn limits() -> OrchestratorLimits {
    OrchestratorLimits::new(8, 8, 8, 8, 8, 32, 32, 64, 16, 32, 65_536, 262_144)
        .expect("fixture limits are valid")
}

fn candidate(
    quality_snapshot: Sha256Digest,
) -> Result<CandidateBinding, peritus_orchestrator::OrchestratorError> {
    CandidateBinding::new(
        revision(),
        SnapshotId::new(bytes(6)).expect("snapshot id is nonzero"),
        digest(7),
        digest(8),
        quality_snapshot,
        None,
        None,
        vec![ActorId::new(bytes(9)).expect("producer id is nonzero")],
        vec![digest(10)],
        limits(),
    )
}

#[test]
fn quality_snapshot_is_material_and_committed_by_candidate_digest() {
    let first = candidate(digest(11)).expect("first candidate is valid");
    let second = candidate(digest(12)).expect("second candidate is valid");

    assert_eq!(first.quality_snapshot_digest(), digest(11));
    assert_ne!(first.digest(), second.digest());
    assert!(!first.materially_equal(&second));
    assert!(!first.reuses_material(&second));
}

#[test]
fn zero_quality_snapshot_is_rejected() {
    let error = candidate(digest(0)).expect_err("quality snapshot digest must be nonzero");
    assert_eq!(error.kind(), OrchestratorErrorKind::NonCanonical);
}
