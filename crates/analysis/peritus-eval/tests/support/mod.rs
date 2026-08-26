#![allow(dead_code, reason = "shared integration fixtures are consumed by different test crates")]

use peritus_artifact_store::ArtifactDigest;
use peritus_eval::{
    CandidateObservation, DatasetId, DatasetManifest, DatasetPartition, DatasetTask,
    EvaluationCampaignId, EvaluationLimits, EvaluationRetryPolicy, EvaluatorObservation,
    EvaluatorVerdict, ExecutionBinding, ExecutionFailure, FrozenEvaluationProfile,
    FrozenModelControls, FrozenProviderSnapshot, HarnessArmBinding, InfrastructurePolicy,
    InfrastructureTreatment, MetricPolicy, ResourceObservation, RolloutExecutionPort,
    SealedEvaluatorInput, SeedDeliveryPolicy, TaskId,
};
use peritus_harness::domain::{HarnessRevisionIdentity, RevisionDigest};
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_scheduler::{ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};

pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
pub const fn artifact(value: u8) -> ArtifactDigest {
    ArtifactDigest::from_sha256(digest(value))
}

pub fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(1)).expect("acceptance"),
        HarnessId::new(bytes(2)).expect("harness"),
        WorkspaceId::new(bytes(3)).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(4)).expect("policy"),
        ProviderProfileId::new(bytes(5)).expect("provider"),
    )
}

pub fn provider_profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new(bytes(5)).expect("profile ID"),
        7,
        ProviderName::new("fixture-provider".to_owned()).expect("provider name"),
        ModelName::new("fixture-model-v1".to_owned()).expect("model name"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::SamplingControls, Capability::UsageDetail], &[])
            .expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(100_000, 16_384, 64, 8, 4 * 1024 * 1024).expect("model limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::Unsupported,
    )
    .expect("provider profile")
}

pub fn dataset() -> DatasetManifest {
    let limits = EvaluationLimits::production();
    let first = DatasetTask::new(
        TaskId::new(bytes(10)).expect("task"),
        DatasetPartition::Regression,
        1,
        peritus_eval::CandidateTaskInput::new(artifact(11), 100).expect("candidate input"),
        SealedEvaluatorInput::new(artifact(21), 80, digest(31)).expect("evaluator input"),
        digest(41),
    )
    .expect("first task");
    let second = DatasetTask::new(
        TaskId::new(bytes(12)).expect("task"),
        DatasetPartition::Regression,
        1,
        peritus_eval::CandidateTaskInput::new(artifact(13), 110).expect("candidate input"),
        SealedEvaluatorInput::new(artifact(23), 90, digest(33)).expect("evaluator input"),
        digest(43),
    )
    .expect("second task");
    DatasetManifest::new(
        DatasetId::new(bytes(9)).expect("dataset"),
        1,
        vec![first, second],
        digest(44),
        limits,
    )
    .expect("dataset manifest")
}

pub fn frozen_profile() -> FrozenEvaluationProfile {
    let limits = EvaluationLimits::production();
    let provider = FrozenProviderSnapshot::capture(&provider_profile()).expect("provider snapshot");
    let model = FrozenModelControls::new(
        4_096,
        Some(0),
        Some(1_000_000),
        digest(50),
        SeedDeliveryPolicy::Required,
        provider,
    )
    .expect("model controls");
    let resources = ResourceVector::new(
        vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).expect("quantity"))],
        4,
    )
    .expect("resources");
    let execution = ExecutionBinding::new(
        digest(51),
        digest(52),
        digest(53),
        digest(54),
        resources,
        60_000_000,
        2,
        true,
        true,
    )
    .expect("execution binding");
    let shared = revision();
    let harness_id = shared.harness_id();
    let baseline = HarnessArmBinding::new(
        shared,
        HarnessRevisionIdentity::new(
            harness_id,
            RevisionNumber::first(),
            RevisionDigest::new(digest(60)),
        ),
        digest(61),
    );
    let candidate = HarnessArmBinding::new(
        shared,
        HarnessRevisionIdentity::new(
            harness_id,
            RevisionNumber::new(2).expect("revision"),
            RevisionDigest::new(digest(62)),
        ),
        digest(63),
    );
    FrozenEvaluationProfile::new(
        dataset(),
        vec![DatasetPartition::Regression],
        baseline,
        candidate,
        provider,
        model,
        execution,
        EvaluationRetryPolicy::new(3, 10, 100, limits).expect("retry"),
        MetricPolicy::new(vec![1, 2], 100, 950_000, 250_000, true, limits).expect("metrics"),
        InfrastructurePolicy::new(
            InfrastructureTreatment::CountAsFailure,
            InfrastructureTreatment::CountAsFailure,
            InfrastructureTreatment::ExcludeWithDenominator,
        ),
        2,
        false,
        limits,
    )
    .expect("frozen evaluation profile")
}

pub fn campaign_id() -> EvaluationCampaignId {
    EvaluationCampaignId::new(bytes(70)).expect("campaign")
}

#[derive(Clone, Copy, Debug)]
pub enum PortMode {
    Pass,
    TaskFail,
    CandidateInfrastructure,
    EvaluatorInfrastructure,
}

pub struct FixturePort {
    pub mode: PortMode,
    pub candidate_calls: u32,
    pub evaluator_calls: u32,
}

impl FixturePort {
    pub const fn new(mode: PortMode) -> Self {
        Self { mode, candidate_calls: 0, evaluator_calls: 0 }
    }
}

impl RolloutExecutionPort for FixturePort {
    fn execute_candidate(
        &mut self,
        directive: &peritus_eval::CandidateExecutionDirective,
    ) -> Result<CandidateObservation, ExecutionFailure> {
        self.candidate_calls += 1;
        if matches!(self.mode, PortMode::CandidateInfrastructure) {
            return Err(ExecutionFailure::new(
                peritus_eval::InfrastructureFailureClass::Provider,
                digest(80),
                true,
            ));
        }
        CandidateObservation::new(
            directive.rollout_id(),
            directive.attempt(),
            directive.request_digest(),
            artifact(81),
            32,
            directive.execution().digest(),
            directive.provider().digest(),
            ResourceObservation::new(
                100,
                Some(10),
                Some(5),
                Some(20),
                Some(1_024),
                Some(50),
                Some(1),
                true,
                true,
            ),
        )
        .map_err(|_| {
            ExecutionFailure::new(
                peritus_eval::InfrastructureFailureClass::Execution,
                digest(82),
                false,
            )
        })
    }

    fn execute_evaluator(
        &mut self,
        directive: &peritus_eval::EvaluatorExecutionDirective,
    ) -> Result<EvaluatorObservation, ExecutionFailure> {
        self.evaluator_calls += 1;
        if matches!(self.mode, PortMode::EvaluatorInfrastructure) {
            return Err(ExecutionFailure::new(
                peritus_eval::InfrastructureFailureClass::Evaluator,
                digest(83),
                true,
            ));
        }
        let verdict = if matches!(self.mode, PortMode::TaskFail) {
            EvaluatorVerdict::Failed(peritus_eval::TaskFailureClass::Incorrect)
        } else {
            EvaluatorVerdict::Passed
        };
        EvaluatorObservation::new(
            directive.rollout_id(),
            directive.attempt(),
            directive.request_digest(),
            directive.candidate_output(),
            verdict,
            digest(84),
            directive.execution().digest(),
            ResourceObservation::new(
                20,
                Some(1),
                Some(1),
                Some(1),
                Some(512),
                Some(10),
                Some(1),
                true,
                true,
            ),
        )
        .map_err(|_| {
            ExecutionFailure::new(
                peritus_eval::InfrastructureFailureClass::Evaluator,
                digest(85),
                false,
            )
        })
    }
}
