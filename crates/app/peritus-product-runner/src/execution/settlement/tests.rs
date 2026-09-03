use super::*;
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, EvidenceRecord, QualificationEvidence,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

#[test]
fn incomplete_candidate_names_each_missing_acceptance_boundary() {
    let identity = CandidateIdentity::new(
        RunId::new([1; 16]).expect("run"),
        WorkspaceId::new([2; 16]).expect("workspace"),
        Sha256Digest::new([3; 32]),
        1,
        1,
    )
    .expect("identity");
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("checkpoint");

    let remaining = remaining_work(Some(&checkpoint), SettlementCause::Provider);

    assert_eq!(remaining.len(), 4);
    assert!(remaining.iter().any(|item| item.contains("deterministic gates")));
    assert!(remaining.iter().any(|item| item.contains("public requirement")));
    assert!(remaining.iter().any(|item| item.contains("independent")));
    assert!(remaining.iter().any(|item| item.contains("provider")));
}

#[test]
fn qualified_candidate_has_no_remaining_work() {
    let identity = CandidateIdentity::new(
        RunId::new([1; 16]).expect("run"),
        WorkspaceId::new([2; 16]).expect("workspace"),
        Sha256Digest::new([3; 32]),
        1,
        1,
    )
    .expect("identity");
    let satisfied =
        EvidenceStatus::Current(EvidenceRecord::new(identity, QualificationEvidence::Satisfied));
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Qualified,
        satisfied,
        satisfied,
        satisfied,
    )
    .expect("checkpoint");

    assert!(remaining_work(Some(&checkpoint), SettlementCause::Completed).is_empty());
}
