//! Public run-settlement transition and failure matrix.

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence, RunDisposition, SettlementCause, SettlementErrorKind, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

fn identity(digest: u8, revision: u64, sequence: u64) -> CandidateIdentity {
    CandidateIdentity::new(
        RunId::new([1; 16]).expect("run"),
        WorkspaceId::new([2; 16]).expect("workspace"),
        Sha256Digest::new([digest; 32]),
        revision,
        sequence,
    )
    .expect("candidate identity")
}

const fn status(
    candidate: CandidateIdentity,
    evidence: QualificationEvidence,
) -> EvidenceStatus<QualificationEvidence> {
    EvidenceStatus::Current(EvidenceRecord::new(candidate, evidence))
}

fn qualified(candidate: CandidateIdentity) -> CandidateCheckpoint {
    CandidateCheckpoint::new(
        candidate,
        CandidateStage::Qualified,
        status(candidate, QualificationEvidence::Satisfied),
        status(candidate, QualificationEvidence::Satisfied),
        status(candidate, QualificationEvidence::Satisfied),
    )
    .expect("qualified checkpoint")
}

#[test]
fn accepted_requires_a_fully_qualified_current_candidate() {
    let candidate = identity(3, 7, 1);
    let mut reducer = SettlementReducer::new();
    reducer.observe(qualified(candidate)).expect("checkpoint");
    let settlement = reducer.settle(SettlementCause::Completed).expect("settlement");

    assert_eq!(settlement.disposition(), RunDisposition::Accepted);
    assert!(settlement.is_accepted());
    assert_eq!(settlement.checkpoint().expect("candidate").identity(), &candidate);

    for (gates, obligations, review) in [
        (
            EvidenceStatus::Missing,
            status(candidate, QualificationEvidence::Satisfied),
            status(candidate, QualificationEvidence::Satisfied),
        ),
        (
            status(candidate, QualificationEvidence::Satisfied),
            EvidenceStatus::Missing,
            status(candidate, QualificationEvidence::Satisfied),
        ),
        (
            status(candidate, QualificationEvidence::Satisfied),
            status(candidate, QualificationEvidence::Satisfied),
            status(candidate, QualificationEvidence::Unsatisfied),
        ),
    ] {
        assert_eq!(
            CandidateCheckpoint::new(
                candidate,
                CandidateStage::Qualified,
                gates,
                obligations,
                review,
            )
            .expect_err("incomplete qualification")
            .kind(),
            SettlementErrorKind::CandidateStageEvidenceMismatch,
        );
    }
}

#[test]
fn candidate_is_delivered_without_being_accepted() {
    let candidate = identity(3, 7, 1);
    let checkpoint = CandidateCheckpoint::new(
        candidate,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("candidate checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("checkpoint");
    let settlement = reducer.settle(SettlementCause::Provider).expect("settlement");

    assert_eq!(settlement.disposition(), RunDisposition::CandidateAvailable);
    assert!(!settlement.is_accepted());
    assert_eq!(settlement.cause(), SettlementCause::Provider);
}

#[test]
fn every_terminal_cause_is_expressible_with_and_without_a_candidate() {
    for cause in [
        SettlementCause::Completed,
        SettlementCause::UserWait,
        SettlementCause::Cancellation,
        SettlementCause::Deadline,
        SettlementCause::Provider,
        SettlementCause::Context,
        SettlementCause::Gate,
        SettlementCause::Review,
        SettlementCause::Repository,
        SettlementCause::Adapter,
        SettlementCause::Recovery,
        SettlementCause::InternalInvariant,
    ] {
        let mut empty = SettlementReducer::new();
        let empty_result = empty.settle(cause).expect("empty settlement");
        assert!(empty_result.checkpoint().is_none());
        assert_eq!(
            empty_result.disposition(),
            match cause {
                SettlementCause::UserWait => RunDisposition::WaitingForUser,
                SettlementCause::Cancellation => RunDisposition::Cancelled,
                SettlementCause::Recovery => RunDisposition::RecoveryRequired,
                _ => RunDisposition::FailedNoCandidate,
            },
        );

        let candidate = identity(3, 7, 1);
        let checkpoint = CandidateCheckpoint::new(
            candidate,
            CandidateStage::Observed,
            EvidenceStatus::Missing,
            EvidenceStatus::Missing,
            EvidenceStatus::Missing,
        )
        .expect("candidate checkpoint");
        let mut with_candidate = SettlementReducer::new();
        with_candidate.observe(checkpoint).expect("checkpoint");
        let candidate_result = with_candidate.settle(cause).expect("candidate settlement");
        assert!(candidate_result.checkpoint().is_some());
        assert_eq!(
            candidate_result.disposition(),
            match cause {
                SettlementCause::UserWait => RunDisposition::WaitingForUser,
                SettlementCause::Cancellation => RunDisposition::Cancelled,
                SettlementCause::Recovery => RunDisposition::RecoveryRequired,
                _ => RunDisposition::CandidateAvailable,
            },
        );
    }
}

#[test]
fn evidence_from_another_candidate_is_rejected_or_stale() {
    let current = identity(4, 8, 2);
    for old in [identity(4, 7, 1), identity(3, 8, 1)] {
        let foreign = status(old, QualificationEvidence::Satisfied);
        assert_eq!(
            CandidateCheckpoint::new(
                current,
                CandidateStage::Changed,
                foreign,
                EvidenceStatus::Missing,
                EvidenceStatus::Missing,
            )
            .expect_err("foreign current evidence")
            .kind(),
            SettlementErrorKind::CurrentEvidenceBindingMismatch,
        );

        CandidateCheckpoint::new(
            current,
            CandidateStage::Changed,
            EvidenceStatus::Stale(EvidenceRecord::new(old, QualificationEvidence::Satisfied)),
            EvidenceStatus::Missing,
            EvidenceStatus::Missing,
        )
        .expect("old evidence is validly stale");
    }
}

#[test]
fn reducer_rejects_nonadvancing_regressing_and_post_terminal_updates() {
    let first = identity(3, 7, 1);
    let second = identity(3, 7, 2);
    let mut reducer = SettlementReducer::new();
    reducer.observe(qualified(first)).expect("first checkpoint");

    let regressed = CandidateCheckpoint::new(
        second,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("well-formed checkpoint");
    assert_eq!(
        reducer.observe(regressed).expect_err("stage regression").kind(),
        SettlementErrorKind::CandidateStageRegressed,
    );

    reducer.settle(SettlementCause::Completed).expect("settlement");
    assert_eq!(
        reducer.settle(SettlementCause::Provider).expect_err("duplicate settlement").kind(),
        SettlementErrorKind::AlreadySettled,
    );
    assert_eq!(
        reducer.observe(qualified(second)).expect_err("post-terminal observation").kind(),
        SettlementErrorKind::AlreadySettled,
    );
}

#[test]
fn cancellation_recovery_and_user_wait_retain_a_qualified_candidate_without_accepting_it() {
    let checkpoint = qualified(identity(3, 7, 1));
    for (cause, expected) in [
        (SettlementCause::Cancellation, RunDisposition::Cancelled),
        (SettlementCause::Recovery, RunDisposition::RecoveryRequired),
        (SettlementCause::UserWait, RunDisposition::WaitingForUser),
    ] {
        let mut reducer = SettlementReducer::new();
        reducer.observe(checkpoint).expect("qualified candidate");
        let result = reducer.settle(cause).expect("terminal override");
        assert_eq!(result.disposition(), expected);
        assert_eq!(result.checkpoint(), Some(&checkpoint));
        assert_eq!(result.cause(), cause);
        assert!(!result.is_accepted());
    }
}

#[test]
fn rejected_observations_and_terminal_updates_preserve_the_complete_reducer() {
    let first = qualified(identity(3, 7, 1));
    let mut reducer = SettlementReducer::new();
    reducer.observe(first).expect("first candidate");
    let before = reducer;
    assert_eq!(
        reducer.observe(first).expect_err("sequence must advance").kind(),
        SettlementErrorKind::CheckpointDidNotAdvance,
    );
    assert_eq!(reducer, before);
    let regressed = CandidateCheckpoint::new(
        identity(3, 7, 2),
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("well-formed earlier stage");
    assert_eq!(
        reducer.observe(regressed).expect_err("stage must not regress").kind(),
        SettlementErrorKind::CandidateStageRegressed,
    );
    assert_eq!(reducer, before);
    reducer.settle(SettlementCause::Completed).expect("first settlement");
    let settled = reducer;
    assert_eq!(
        reducer.settle(SettlementCause::Cancellation).expect_err("already terminal").kind(),
        SettlementErrorKind::AlreadySettled,
    );
    assert_eq!(reducer, settled);
    assert_eq!(
        reducer.observe(qualified(identity(4, 8, 3))).expect_err("already terminal").kind(),
        SettlementErrorKind::AlreadySettled,
    );
    assert_eq!(reducer, settled);
}

#[test]
fn candidate_comparison_uses_every_identity_byte_and_ignores_only_checkpoint_sequence() {
    let candidate = identity(3, 7, 1);
    let advanced = identity(3, 7, 2);
    assert!(candidate.same_candidate(&advanced));
    assert!(!candidate.same_candidate(&identity(3, 8, 2)));
    for index in 0..32 {
        let mut digest = [3; 32];
        digest[index] = 4;
        let changed = CandidateIdentity::new(
            candidate.run_id(),
            candidate.workspace_id(),
            Sha256Digest::new(digest),
            7,
            2,
        )
        .expect("changed content");
        assert!(candidate.same_lineage(&changed));
        assert!(!candidate.same_candidate(&changed));
    }
    for index in 0..16 {
        let mut run = [1; 16];
        run[index] = 3;
        let mut workspace = [2; 16];
        workspace[index] = 3;
        for (run_id, workspace_id) in [
            (RunId::new(run).expect("changed run"), candidate.workspace_id()),
            (candidate.run_id(), WorkspaceId::new(workspace).expect("changed workspace")),
        ] {
            let changed =
                CandidateIdentity::new(run_id, workspace_id, candidate.candidate_digest(), 7, 2)
                    .expect("changed lineage");
            assert!(!candidate.same_lineage(&changed));
            assert!(!candidate.same_candidate(&changed));
        }
    }
}

#[test]
fn every_stable_tag_round_trips_and_unknown_tags_reject() {
    for stage in [
        CandidateStage::Observed,
        CandidateStage::Changed,
        CandidateStage::SelfChecked,
        CandidateStage::GatesPassed,
        CandidateStage::ReviewPending,
        CandidateStage::Qualified,
    ] {
        assert_eq!(CandidateStage::from_tag(stage.tag()), Some(stage));
    }
    for disposition in [
        RunDisposition::Accepted,
        RunDisposition::CandidateAvailable,
        RunDisposition::WaitingForUser,
        RunDisposition::FailedNoCandidate,
        RunDisposition::Cancelled,
        RunDisposition::RecoveryRequired,
    ] {
        assert_eq!(RunDisposition::from_tag(disposition.tag()), Some(disposition));
    }
    for cause in [
        SettlementCause::Completed,
        SettlementCause::UserWait,
        SettlementCause::Cancellation,
        SettlementCause::Deadline,
        SettlementCause::Provider,
        SettlementCause::Context,
        SettlementCause::Gate,
        SettlementCause::Review,
        SettlementCause::Repository,
        SettlementCause::Adapter,
        SettlementCause::Recovery,
        SettlementCause::InternalInvariant,
    ] {
        assert_eq!(SettlementCause::from_tag(cause.tag()), Some(cause));
    }
    assert_eq!(CandidateStage::from_tag(0), None);
    assert_eq!(RunDisposition::from_tag(u16::MAX), None);
    assert_eq!(SettlementCause::from_tag(13), None);
    for evidence in [QualificationEvidence::Satisfied, QualificationEvidence::Unsatisfied] {
        assert_eq!(QualificationEvidence::from_tag(evidence.tag()), Some(evidence));
    }
    assert_eq!(QualificationEvidence::from_tag(0), None);

    let candidate = identity(3, 7, 1);
    let record = EvidenceRecord::new(candidate, QualificationEvidence::Satisfied);
    for (evidence, tag) in [
        (EvidenceStatus::Missing, 1),
        (EvidenceStatus::Current(record), 2),
        (EvidenceStatus::Failed(record), 3),
        (EvidenceStatus::Stale(record), 4),
    ] {
        assert_eq!(evidence.tag(), tag);
    }
}
