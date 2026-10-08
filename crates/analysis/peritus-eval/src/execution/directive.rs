//! Capability-separated candidate and evaluator directives.

use peritus_artifact_store::ArtifactDigest;
use peritus_types::Sha256Digest;

use crate::{
    CandidateTaskInput, EvaluationArm, EvaluationCampaignId, ExecutionBinding,
    FrozenEvaluationProfile, FrozenModelControls, FrozenProviderSnapshot, HarnessArmBinding,
    ProfileDigest, RolloutId, RolloutSeed, RolloutSpec, SealedEvaluatorInput, TaskId,
};

const STAGE_DOMAIN: &[u8] = b"peritus.evaluation.execution-stage.v1\0";
const CONTINUATION_DOMAIN: &[u8] = b"peritus.evaluation.execution-continuation.v1\0";

/// Closed candidate/evaluator stage identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExecutionStage {
    /// Candidate-visible generation stage.
    Candidate,
    /// Separately authorized evaluator stage.
    Evaluator,
}

impl ExecutionStage {
    const fn tag(self) -> u8 {
        match self {
            Self::Candidate => 1,
            Self::Evaluator => 2,
        }
    }
}

/// Exact identity used by an owned, interruptible stage across polls and process recovery.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionStageIdentity {
    rollout_id: RolloutId,
    attempt: u16,
    stage: ExecutionStage,
    request_digest: Sha256Digest,
    candidate_output: Option<ArtifactDigest>,
    candidate_output_bytes: Option<u64>,
    digest: Sha256Digest,
}

impl ExecutionStageIdentity {
    fn new(
        rollout_id: RolloutId,
        attempt: u16,
        stage: ExecutionStage,
        request_digest: Sha256Digest,
        candidate_output: Option<ArtifactDigest>,
        candidate_output_bytes: Option<u64>,
    ) -> Self {
        let mut bytes = Vec::with_capacity(192);
        bytes.extend_from_slice(STAGE_DOMAIN);
        bytes.extend_from_slice(rollout_id.as_bytes());
        bytes.extend_from_slice(&attempt.to_be_bytes());
        bytes.push(stage.tag());
        bytes.extend_from_slice(request_digest.as_bytes());
        bytes.push(u8::from(candidate_output.is_some()));
        if let Some(value) = candidate_output {
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes.push(u8::from(candidate_output_bytes.is_some()));
        if let Some(value) = candidate_output_bytes {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        let digest = peritus_codec::sha256(&bytes);
        Self {
            rollout_id,
            attempt,
            stage,
            request_digest,
            candidate_output,
            candidate_output_bytes,
            digest,
        }
    }
    /// Logical rollout whose owned process is executing.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Exact logical attempt.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Candidate or evaluator stage.
    #[must_use]
    pub const fn stage(self) -> ExecutionStage {
        self.stage
    }
    /// Complete frozen request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Candidate artifact bound to evaluator work, when applicable.
    #[must_use]
    pub const fn candidate_output(self) -> Option<ArtifactDigest> {
        self.candidate_output
    }
    /// Candidate artifact length bound to evaluator work, when applicable.
    #[must_use]
    pub const fn candidate_output_bytes(self) -> Option<u64> {
        self.candidate_output_bytes
    }
    /// Complete stable stage identity.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

/// Opaque resumable checkpoint tied to one exact owned execution stage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionContinuation {
    stage_digest: Sha256Digest,
    checkpoint_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl ExecutionContinuation {
    pub(crate) fn new(stage: ExecutionStageIdentity, checkpoint_digest: Sha256Digest) -> Self {
        let mut bytes = Vec::with_capacity(96);
        bytes.extend_from_slice(CONTINUATION_DOMAIN);
        bytes.extend_from_slice(stage.digest().as_bytes());
        bytes.extend_from_slice(checkpoint_digest.as_bytes());
        let digest = peritus_codec::sha256(&bytes);
        Self { stage_digest: stage.digest(), checkpoint_digest, digest }
    }
    /// Exact stage to resume without creating another job or effect.
    #[must_use]
    pub const fn stage_digest(self) -> Sha256Digest {
        self.stage_digest
    }
    /// Owner-produced checkpoint identity.
    #[must_use]
    pub const fn checkpoint_digest(self) -> Sha256Digest {
        self.checkpoint_digest
    }
    /// Complete continuation identity.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    pub(crate) const fn belongs_to(self, stage: ExecutionStageIdentity) -> bool {
        self.stage_digest == stage.digest()
    }
}

/// Complete candidate-visible directive for one rollout attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateExecutionDirective {
    rollout_id: RolloutId,
    campaign_id: EvaluationCampaignId,
    profile_digest: ProfileDigest,
    task_id: TaskId,
    arm: EvaluationArm,
    attempt: u16,
    seed: RolloutSeed,
    request_digest: Sha256Digest,
    input: CandidateTaskInput,
    harness: HarnessArmBinding,
    provider: FrozenProviderSnapshot,
    model: FrozenModelControls,
    execution: ExecutionBinding,
}

impl CandidateExecutionDirective {
    /// Builds an exact candidate-only view from a frozen plan and profile.
    ///
    /// # Errors
    /// Rejects attempt zero or a spec/profile mismatch.
    pub fn from_frozen(
        spec: &RolloutSpec,
        profile: &FrozenEvaluationProfile,
        attempt: u16,
    ) -> Result<Self, crate::EvaluationError> {
        if attempt == 0
            || spec.profile_digest() != profile.digest()
            || spec.public_input().artifact()
                != profile
                    .dataset()
                    .tasks()
                    .iter()
                    .find(|task| task.id() == spec.task_id())
                    .map(|task| task.candidate_input().artifact())
                    .ok_or_else(binding)?
        {
            return Err(binding());
        }
        Ok(Self {
            rollout_id: spec.id(),
            campaign_id: spec.campaign_id(),
            profile_digest: spec.profile_digest(),
            task_id: spec.task_id(),
            arm: spec.arm(),
            attempt,
            seed: spec.seed(),
            request_digest: spec.request_digest(),
            input: spec.public_input(),
            harness: profile.arm(spec.arm()),
            provider: profile.provider(),
            model: profile.model(),
            execution: profile.execution().clone(),
        })
    }

    /// Logical rollout identity.
    #[must_use]
    pub const fn rollout_id(&self) -> RolloutId {
        self.rollout_id
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Frozen profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> ProfileDigest {
        self.profile_digest
    }
    /// Task identity.
    #[must_use]
    pub const fn task_id(&self) -> TaskId {
        self.task_id
    }
    /// Evaluation arm.
    #[must_use]
    pub const fn arm(&self) -> EvaluationArm {
        self.arm
    }
    /// One-based attempt.
    #[must_use]
    pub const fn attempt(&self) -> u16 {
        self.attempt
    }
    /// Shared paired seed.
    #[must_use]
    pub const fn seed(&self) -> RolloutSeed {
        self.seed
    }
    /// Complete planned request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
    /// Candidate-visible public input.
    #[must_use]
    pub const fn input(&self) -> CandidateTaskInput {
        self.input
    }
    /// Exact E1 harness binding.
    #[must_use]
    pub const fn harness(&self) -> HarnessArmBinding {
        self.harness
    }
    /// Frozen C5 profile snapshot.
    #[must_use]
    pub const fn provider(&self) -> FrozenProviderSnapshot {
        self.provider
    }
    /// Frozen model controls.
    #[must_use]
    pub const fn model(&self) -> FrozenModelControls {
        self.model
    }
    /// Frozen C2/C3 requirements.
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    /// Optional caller-selected wall deadline for the candidate stage.
    #[must_use]
    pub const fn deadline_micros(&self) -> Option<u64> {
        self.execution.deadline_micros()
    }
    /// Stable owned-stage identity used for cancellation polling and exact resumption.
    #[must_use]
    pub fn stage_identity(&self) -> ExecutionStageIdentity {
        ExecutionStageIdentity::new(
            self.rollout_id,
            self.attempt,
            ExecutionStage::Candidate,
            self.request_digest,
            None,
            None,
        )
    }
}

/// Separately authorized evaluator-only directive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluatorExecutionDirective {
    rollout_id: RolloutId,
    attempt: u16,
    request_digest: Sha256Digest,
    candidate_output: ArtifactDigest,
    candidate_output_bytes: u64,
    sealed_input: SealedEvaluatorInput,
    execution: ExecutionBinding,
}

impl EvaluatorExecutionDirective {
    pub(crate) fn from_candidate(
        candidate: &CandidateExecutionDirective,
        profile: &FrozenEvaluationProfile,
        candidate_output: ArtifactDigest,
        candidate_output_bytes: u64,
    ) -> Result<Self, crate::EvaluationError> {
        let task = profile
            .dataset()
            .tasks()
            .iter()
            .find(|task| task.id() == candidate.task_id())
            .ok_or_else(binding)?;
        if candidate_output_bytes == 0
            || candidate.profile_digest() != profile.digest()
            || task.evaluator_input().verifier_digest()
                != profile
                    .dataset()
                    .tasks()
                    .iter()
                    .find(|value| value.id() == candidate.task_id())
                    .map(|value| value.evaluator_input().verifier_digest())
                    .ok_or_else(binding)?
        {
            return Err(binding());
        }
        Ok(Self {
            rollout_id: candidate.rollout_id(),
            attempt: candidate.attempt(),
            request_digest: candidate.request_digest(),
            candidate_output,
            candidate_output_bytes,
            sealed_input: task.evaluator_input(),
            execution: profile.execution().clone(),
        })
    }

    /// Logical rollout identity.
    #[must_use]
    pub const fn rollout_id(&self) -> RolloutId {
        self.rollout_id
    }
    /// One-based attempt.
    #[must_use]
    pub const fn attempt(&self) -> u16 {
        self.attempt
    }
    /// Complete planned request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
    /// Finalized candidate output artifact.
    #[must_use]
    pub const fn candidate_output(&self) -> ArtifactDigest {
        self.candidate_output
    }
    /// Exact candidate output byte count.
    #[must_use]
    pub const fn candidate_output_bytes(&self) -> u64 {
        self.candidate_output_bytes
    }
    /// Evaluator-only hidden input.
    #[must_use]
    pub const fn sealed_input(&self) -> SealedEvaluatorInput {
        self.sealed_input
    }
    /// Frozen C2/C3 requirements.
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    /// Optional caller-selected wall deadline for the evaluator stage.
    #[must_use]
    pub const fn deadline_micros(&self) -> Option<u64> {
        self.execution.deadline_micros()
    }
    /// Stable owned-stage identity, including the exact candidate artifact under evaluation.
    #[must_use]
    pub fn stage_identity(&self) -> ExecutionStageIdentity {
        ExecutionStageIdentity::new(
            self.rollout_id,
            self.attempt,
            ExecutionStage::Evaluator,
            self.request_digest,
            Some(self.candidate_output),
            Some(self.candidate_output_bytes),
        )
    }
}

const fn binding() -> crate::EvaluationError {
    crate::invalid(
        crate::EvaluationErrorKind::Binding,
        crate::EvaluationOperation::Execute,
        "execution directive differs from the frozen plan or profile",
    )
}
