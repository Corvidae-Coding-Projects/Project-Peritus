//! Reconciliation retains facts while qualification follows current observations.

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence, SettlementErrorKind,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

fn identity(digest: u8, sequence: u64) -> CandidateIdentity {
    CandidateIdentity::new(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        Sha256Digest::new([digest; 32]),
        3,
        sequence,
    )
    .unwrap()
}

const fn positive(identity: CandidateIdentity) -> EvidenceStatus<QualificationEvidence> {
    EvidenceStatus::Current(EvidenceRecord::new(identity, QualificationEvidence::Satisfied))
}

#[test]
fn unchanged_candidate_preserves_provenance_but_new_review_failure_revokes_qualification() {
    let original = identity(4, 1);
    let prior = CandidateCheckpoint::new(
        original,
        CandidateStage::Qualified,
        positive(original),
        positive(original),
        positive(original),
    )
    .unwrap();
    let current_identity = identity(4, 2);
    let failed_review = EvidenceStatus::Failed(EvidenceRecord::new(
        current_identity,
        QualificationEvidence::Unsatisfied,
    ));
    let current = CandidateCheckpoint::observe(
        current_identity,
        CandidateStage::Qualified,
        *prior.gates(),
        *prior.obligations(),
        failed_review,
    )
    .unwrap();
    current.validate_successor(&prior).unwrap();
    assert_eq!(current.stage(), CandidateStage::GatesPassed);
    assert_eq!(current.gates().record().unwrap().provenance(), &original);
    assert_eq!(current.review(), &failed_review);
    assert!(!current.is_qualified());
}

#[test]
fn exact_reversion_reconciles_retained_observations_without_rewriting_their_origin() {
    let original = identity(4, 1);
    let prior = CandidateCheckpoint::new(
        original,
        CandidateStage::Qualified,
        positive(original),
        positive(original),
        positive(original),
    )
    .unwrap();
    let changed = prior.reobserve(identity(5, 2)).unwrap();
    assert!(matches!(changed.gates(), EvidenceStatus::Stale(_)));
    assert_eq!(changed.stage(), CandidateStage::Changed);
    let reverted = changed.reobserve(identity(4, 3)).unwrap();
    assert_eq!(reverted.gates(), prior.gates());
    assert_eq!(reverted.obligations(), prior.obligations());
    assert_eq!(reverted.review(), prior.review());
    assert_eq!(reverted.stage(), CandidateStage::Changed);
    assert!(!reverted.is_qualified());
}

#[test]
fn matching_future_evidence_and_nonadvancing_reobservations_are_rejected() {
    let current = identity(4, 1);
    assert_eq!(
        CandidateCheckpoint::observe(
            current,
            CandidateStage::GatesPassed,
            positive(identity(4, 2)),
            EvidenceStatus::Missing,
            EvidenceStatus::Missing,
        )
        .unwrap_err()
        .kind(),
        SettlementErrorKind::CurrentEvidenceBindingMismatch,
    );
    let prior = CandidateCheckpoint::new(
        current,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .unwrap();
    assert_eq!(
        prior.reobserve(current).unwrap_err().kind(),
        SettlementErrorKind::CheckpointDidNotAdvance
    );
}
