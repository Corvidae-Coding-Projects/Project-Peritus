//! Complete immutable logical-terminal records.

use peritus_artifact_store::ArtifactDigest;
use peritus_scheduler::WorkId;
use peritus_types::Sha256Digest;

use crate::{
    EvaluationArm, EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    ExecutedRollout, ProfileDigest, ResourceObservation, RolloutAttempt, RolloutId, RolloutOutcome,
    RolloutSeed, RolloutSpec, TaskId,
};

/// Complete result retained for one expected logical rollout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolloutRecord {
    rollout_id: RolloutId,
    campaign_id: EvaluationCampaignId,
    profile_digest: ProfileDigest,
    task_id: TaskId,
    arm: EvaluationArm,
    ordinal: u16,
    seed: RolloutSeed,
    work_id: WorkId,
    request_digest: Sha256Digest,
    attempt: RolloutAttempt,
    candidate_output: Option<ArtifactDigest>,
    candidate_resources: Option<ResourceObservation>,
    evaluator_resources: Option<ResourceObservation>,
    trace_root: Option<ArtifactDigest>,
    evidence_root: Option<ArtifactDigest>,
    digest: Sha256Digest,
}

impl RolloutRecord {
    /// Binds a checked execution result to every frozen rollout identity.
    ///
    /// # Errors
    /// Rejects cross-rollout observations or inconsistent terminal evidence.
    pub fn from_execution(
        spec: &RolloutSpec,
        executed: ExecutedRollout,
        trace_root: Option<ArtifactDigest>,
        evidence_root: Option<ArtifactDigest>,
    ) -> Result<Self, EvaluationError> {
        if executed.candidate().is_some_and(|value| value.rollout_id() != spec.id())
            || executed.evaluator().is_some_and(|value| value.rollout_id() != spec.id())
            || executed.attempt().number() == 0
        {
            return Err(invalid("terminal observations differ from the rollout identity"));
        }
        let candidate_output = executed.candidate().map(crate::CandidateObservation::output);
        let candidate_resources = executed.candidate().map(crate::CandidateObservation::resources);
        let evaluator_resources = executed.evaluator().map(crate::EvaluatorObservation::resources);
        if executed.evaluator().is_some() && candidate_output.is_none() {
            return Err(invalid("evaluator observation exists without candidate output"));
        }
        let mut record = Self {
            rollout_id: spec.id(),
            campaign_id: spec.campaign_id(),
            profile_digest: spec.profile_digest(),
            task_id: spec.task_id(),
            arm: spec.arm(),
            ordinal: spec.ordinal(),
            seed: spec.seed(),
            work_id: spec.work_id(),
            request_digest: spec.request_digest(),
            attempt: executed.attempt(),
            candidate_output,
            candidate_resources,
            evaluator_resources,
            trace_root,
            evidence_root,
            digest: Sha256Digest::new([0; 32]),
        };
        record.digest = peritus_codec::sha256(&record.identity_bytes());
        Ok(record)
    }

    /// Logical rollout identity.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Frozen profile digest.
    #[must_use]
    pub const fn profile_digest(self) -> ProfileDigest {
        self.profile_digest
    }
    /// Task identity.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
    /// Evaluation arm.
    #[must_use]
    pub const fn arm(self) -> EvaluationArm {
        self.arm
    }
    /// Paired ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u16 {
        self.ordinal
    }
    /// Paired seed.
    #[must_use]
    pub const fn seed(self) -> RolloutSeed {
        self.seed
    }
    /// D3 work identity.
    #[must_use]
    pub const fn work_id(self) -> WorkId {
        self.work_id
    }
    /// Complete execution request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Retained terminal attempt.
    #[must_use]
    pub const fn attempt(self) -> RolloutAttempt {
        self.attempt
    }
    /// Logical terminal outcome.
    #[must_use]
    pub const fn outcome(self) -> RolloutOutcome {
        self.attempt.terminal()
    }
    /// Finalized candidate output when produced.
    #[must_use]
    pub const fn candidate_output(self) -> Option<ArtifactDigest> {
        self.candidate_output
    }
    /// Candidate-stage resources when execution started.
    #[must_use]
    pub const fn candidate_resources(self) -> Option<ResourceObservation> {
        self.candidate_resources
    }
    /// Evaluator-stage resources when it completed.
    #[must_use]
    pub const fn evaluator_resources(self) -> Option<ResourceObservation> {
        self.evaluator_resources
    }
    /// Complete trace root when available.
    #[must_use]
    pub const fn trace_root(self) -> Option<ArtifactDigest> {
        self.trace_root
    }
    /// Per-rollout evidence root when available.
    #[must_use]
    pub const fn evidence_root(self) -> Option<ArtifactDigest> {
        self.evidence_root
    }
    /// Digest binding every record field.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }

    fn identity_bytes(self) -> Vec<u8> {
        let mut bytes = b"peritus.evaluation.rollout-record.v1\0".to_vec();
        bytes.extend_from_slice(self.rollout_id.as_bytes());
        bytes.extend_from_slice(self.campaign_id.as_bytes());
        bytes.extend_from_slice(self.profile_digest.as_bytes());
        bytes.extend_from_slice(self.task_id.as_bytes());
        bytes.push(self.arm.tag());
        bytes.extend_from_slice(&self.ordinal.to_be_bytes());
        bytes.extend_from_slice(self.seed.as_bytes());
        bytes.extend_from_slice(self.work_id.as_bytes());
        bytes.extend_from_slice(self.request_digest.as_bytes());
        bytes.extend_from_slice(&self.attempt.number().to_be_bytes());
        bytes.extend_from_slice(self.attempt.observation_digest().as_bytes());
        encode_outcome(&mut bytes, self.outcome());
        encode_artifact(&mut bytes, self.candidate_output);
        encode_resource(&mut bytes, self.candidate_resources);
        encode_resource(&mut bytes, self.evaluator_resources);
        encode_artifact(&mut bytes, self.trace_root);
        encode_artifact(&mut bytes, self.evidence_root);
        bytes
    }
}

fn encode_outcome(bytes: &mut Vec<u8>, outcome: RolloutOutcome) {
    match outcome {
        RolloutOutcome::TaskPassed { evaluator_digest } => {
            bytes.push(1);
            bytes.extend_from_slice(evaluator_digest.as_bytes());
        }
        RolloutOutcome::TaskFailed { class, evaluator_digest } => {
            bytes.push(2);
            bytes.push(match class {
                crate::TaskFailureClass::Incorrect => 1,
                crate::TaskFailureClass::Safety => 2,
                crate::TaskFailureClass::Incomplete => 3,
            });
            bytes.extend_from_slice(evaluator_digest.as_bytes());
        }
        RolloutOutcome::InfrastructureFailed { class, failure_digest, retryable } => {
            bytes.push(3);
            bytes.push(match class {
                crate::InfrastructureFailureClass::Scheduler => 1,
                crate::InfrastructureFailureClass::Execution => 2,
                crate::InfrastructureFailureClass::Provider => 3,
                crate::InfrastructureFailureClass::Evaluator => 4,
                crate::InfrastructureFailureClass::Publication => 5,
            });
            bytes.extend_from_slice(failure_digest.as_bytes());
            bytes.push(u8::from(retryable));
        }
        RolloutOutcome::Cancelled => bytes.push(4),
        RolloutOutcome::Ambiguous { observation_digest } => {
            bytes.push(5);
            bytes.extend_from_slice(observation_digest.as_bytes());
        }
    }
}

fn encode_artifact(bytes: &mut Vec<u8>, value: Option<ArtifactDigest>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(value.as_bytes());
    }
}

fn encode_resource(bytes: &mut Vec<u8>, value: Option<ResourceObservation>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(&value.elapsed_micros().to_be_bytes());
        for item in [
            value.input_tokens(),
            value.output_tokens(),
            value.cost_microunits(),
            value.memory_high_water_bytes(),
            value.cpu_micros(),
        ] {
            bytes.push(u8::from(item.is_some()));
            if let Some(item) = item {
                bytes.extend_from_slice(&item.to_be_bytes());
            }
        }
        bytes.push(u8::from(value.process_high_water().is_some()));
        if let Some(item) = value.process_high_water() {
            bytes.extend_from_slice(&item.to_be_bytes());
        }
        bytes.push(u8::from(value.trace_complete()));
        bytes.push(u8::from(value.teardown_complete()));
    }
}

const fn invalid(detail: &'static str) -> EvaluationError {
    crate::invalid(EvaluationErrorKind::Binding, EvaluationOperation::Account, detail)
}
