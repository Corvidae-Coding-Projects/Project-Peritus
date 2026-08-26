//! Capability-separated candidate and evaluator directives.

use peritus_artifact_store::ArtifactDigest;
use peritus_types::Sha256Digest;

use crate::{
    CandidateTaskInput, EvaluationArm, EvaluationCampaignId, ExecutionBinding,
    FrozenEvaluationProfile, FrozenModelControls, FrozenProviderSnapshot, HarnessArmBinding,
    ProfileDigest, RolloutId, RolloutSeed, RolloutSpec, SealedEvaluatorInput, TaskId,
};

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
}

const fn binding() -> crate::EvaluationError {
    crate::invalid(
        crate::EvaluationErrorKind::Binding,
        crate::EvaluationOperation::Execute,
        "execution directive differs from the frozen plan or profile",
    )
}
