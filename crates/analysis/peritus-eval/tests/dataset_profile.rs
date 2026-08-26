//! Dataset isolation and frozen-profile identity coverage.

mod support;

use peritus_eval::{
    CandidateTaskInput, DatasetId, DatasetManifest, DatasetPartition, DatasetTask,
    EvaluationErrorKind, EvaluationLimits, SealedEvaluatorInput, TaskId,
};

use support::{artifact, bytes, dataset, digest, frozen_profile};

#[test]
fn canonical_dataset_is_reproducible_and_keeps_evaluator_roots_sealed() {
    let first = dataset();
    let second = dataset();
    assert_eq!(first, second);
    assert_eq!(first.digest(), second.digest());
    assert_eq!(
        first.canonical_bytes().expect("canonical dataset"),
        second.canonical_bytes().expect("canonical dataset")
    );
    for task in first.tasks() {
        assert_ne!(task.candidate_input().artifact(), task.evaluator_input().artifact());
    }
}

#[test]
fn cross_corpus_candidate_evaluator_collision_is_rejected() {
    let limits = EvaluationLimits::production();
    let first = DatasetTask::new(
        TaskId::new(bytes(10)).expect("task"),
        DatasetPartition::Regression,
        1,
        CandidateTaskInput::new(artifact(11), 1).expect("candidate"),
        SealedEvaluatorInput::new(artifact(21), 1, digest(31)).expect("evaluator"),
        digest(41),
    )
    .expect("task");
    let second = DatasetTask::new(
        TaskId::new(bytes(12)).expect("task"),
        DatasetPartition::Regression,
        1,
        CandidateTaskInput::new(artifact(21), 1).expect("candidate"),
        SealedEvaluatorInput::new(artifact(22), 1, digest(32)).expect("evaluator"),
        digest(42),
    )
    .expect("task");
    let error = DatasetManifest::new(
        DatasetId::new(bytes(9)).expect("dataset"),
        1,
        vec![first, second],
        digest(50),
        limits,
    )
    .expect_err("cross-corpus leak must reject");
    assert_eq!(error.kind(), EvaluationErrorKind::Isolation);
}

#[test]
fn frozen_profile_and_plan_inputs_are_stable() {
    let left = frozen_profile();
    let right = frozen_profile();
    assert_eq!(left.digest(), right.digest());
    assert_ne!(
        left.arm(peritus_eval::EvaluationArm::Baseline).digest(),
        left.arm(peritus_eval::EvaluationArm::Candidate).digest()
    );
    assert_eq!(left.rollouts_per_task(), 2);
}
