//! Dataset isolation and frozen-profile identity coverage.

mod support;

use peritus_eval::{
    CandidateTaskInput, DatasetId, DatasetManifest, DatasetPartition, DatasetTask,
    EvaluationErrorKind, EvaluationLimits, FrozenProviderSnapshot, SealedEvaluatorInput, TaskId,
};
use peritus_model_protocol::{Capability, CapabilityMatrix, ProviderProfile};

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

#[test]
fn extended_provider_snapshot_binds_supported_and_unknown_reasoning_replay() {
    let profile = support::provider_profile();
    let legacy = FrozenProviderSnapshot::capture(&profile).expect("legacy snapshot");
    let with_capabilities = |supported: &[Capability], unknown: &[Capability]| {
        let mut supported = supported.to_vec();
        supported.extend([Capability::SamplingControls, Capability::UsageDetail]);
        let extended = ProviderProfile::new(
            profile.profile_id(),
            profile.revision(),
            profile.provider().clone(),
            profile.model().clone(),
            profile.dialect(),
            CapabilityMatrix::new(&supported, unknown).expect("capabilities"),
            profile.provenance(),
            profile.limits(),
            profile.output_limit_enforcement(),
            profile.state_mode(),
            profile.resume_kind(),
            profile.cancellation_kind(),
        )
        .expect("extended profile");
        FrozenProviderSnapshot::capture(&extended).expect("extended snapshot")
    };
    let supported = with_capabilities(&[Capability::ReasoningReplay], &[]);
    let unknown = with_capabilities(&[], &[Capability::ReasoningReplay]);
    assert_eq!(with_capabilities(&[], &[]), legacy);
    assert_eq!(with_capabilities(&[Capability::ReasoningReplay], &[]), supported);
    assert_ne!(supported.digest(), legacy.digest());
    assert_ne!(unknown.digest(), legacy.digest());
    assert_ne!(supported.digest(), unknown.digest());
}
