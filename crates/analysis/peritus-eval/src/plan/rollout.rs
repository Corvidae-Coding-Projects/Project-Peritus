//! One immutable logical rollout and its D3 work specification.

use peritus_scheduler::{
    AttemptNumber, ExecutionClass, RecoveryPolicy, SchedulerLimits, WorkId, WorkSpec,
};
use peritus_types::{ActorId, Sha256Digest};

use crate::{
    CandidateTaskInput, DatasetPartition, EvaluationArm, EvaluationCampaignId, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationRecovery, ProfileDigest, RolloutId,
    RolloutSeed, TaskId,
};

/// Deterministic block and within-pair rank used only for dispatch order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchedulingKey {
    block: [u8; 32],
    within_pair: u8,
}

impl SchedulingKey {
    /// Borrows the deterministic task/ordinal block hash.
    #[must_use]
    pub const fn block(&self) -> &[u8; 32] {
        &self.block
    }
    /// Returns zero/one arm position within the pair.
    #[must_use]
    pub const fn within_pair(self) -> u8 {
        self.within_pair
    }
}

/// One exact task/arm/ordinal rollout cell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutSpec {
    id: RolloutId,
    work_id: WorkId,
    campaign_id: EvaluationCampaignId,
    profile_digest: ProfileDigest,
    task_id: TaskId,
    partition: DatasetPartition,
    arm: EvaluationArm,
    ordinal: u16,
    seed: RolloutSeed,
    public_input: CandidateTaskInput,
    evaluator_binding_digest: Sha256Digest,
    request_digest: Sha256Digest,
    scheduling_key: SchedulingKey,
}

impl RolloutSpec {
    #[allow(clippy::too_many_arguments, reason = "the planner binds every immutable rollout field")]
    pub(crate) fn derive(
        campaign_id: EvaluationCampaignId,
        profile_digest: ProfileDigest,
        task_id: TaskId,
        partition: DatasetPartition,
        arm: EvaluationArm,
        ordinal: u16,
        seed: RolloutSeed,
        public_input: CandidateTaskInput,
        evaluator_binding_digest: Sha256Digest,
        arm_binding_digest: Sha256Digest,
        provider_digest: Sha256Digest,
        model_digest: Sha256Digest,
        execution_digest: Sha256Digest,
    ) -> Result<Self, EvaluationError> {
        if ordinal == 0 {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::BuildPlan,
                "rollout ordinal is zero",
            ));
        }
        let arm_tag = [arm.tag()];
        let ordinal_bytes = ordinal.to_be_bytes();
        let id = RolloutId::new(crate::identity::derived_identity(
            b"peritus.evaluation.rollout-id.v1\0",
            &[
                campaign_id.as_bytes(),
                profile_digest.as_bytes(),
                task_id.as_bytes(),
                &arm_tag,
                &ordinal_bytes,
            ],
        ))?;
        let request_digest = request_digest(
            campaign_id,
            profile_digest,
            task_id,
            partition,
            arm,
            ordinal,
            seed,
            public_input,
            evaluator_binding_digest,
            arm_binding_digest,
            provider_digest,
            model_digest,
            execution_digest,
        );
        let work_id = WorkId::new(crate::identity::derived_identity(
            b"peritus.evaluation.work-id.v1\0",
            &[id.as_bytes(), request_digest.as_bytes()],
        ))
        .map_err(|_| {
            EvaluationError::new(
                EvaluationErrorKind::Scheduling,
                EvaluationOperation::BuildPlan,
                EvaluationRecovery::Quarantine,
                "derived D3 work identity is invalid",
            )
        })?;
        let scheduling_key = schedule_key(profile_digest, task_id, ordinal, arm, seed);
        Ok(Self {
            id,
            work_id,
            campaign_id,
            profile_digest,
            task_id,
            partition,
            arm,
            ordinal,
            seed,
            public_input,
            evaluator_binding_digest,
            request_digest,
            scheduling_key,
        })
    }

    /// Rollout identity.
    #[must_use]
    pub const fn id(&self) -> RolloutId {
        self.id
    }
    /// D3 work identity.
    #[must_use]
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    /// Owning E3 campaign.
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
    /// Dataset partition.
    #[must_use]
    pub const fn partition(&self) -> DatasetPartition {
        self.partition
    }
    /// Baseline/candidate arm.
    #[must_use]
    pub const fn arm(&self) -> EvaluationArm {
        self.arm
    }
    /// One-based replicate ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u16 {
        self.ordinal
    }
    /// Shared paired seed.
    #[must_use]
    pub const fn seed(&self) -> RolloutSeed {
        self.seed
    }
    /// Candidate-visible input only.
    #[must_use]
    pub const fn public_input(&self) -> CandidateTaskInput {
        self.public_input
    }
    /// Opaque evaluator schema/binding digest, never hidden roots.
    #[must_use]
    pub const fn evaluator_binding_digest(&self) -> Sha256Digest {
        self.evaluator_binding_digest
    }
    /// Complete inert execution-request digest.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
    /// Deterministic blocked dispatch ordering key.
    #[must_use]
    pub const fn scheduling_key(&self) -> SchedulingKey {
        self.scheduling_key
    }

    /// Constructs exact inert D3 coordination work for this rollout.
    ///
    /// # Errors
    /// Rejects incompatible scheduler limits or resources.
    pub fn work_spec(
        &self,
        owner: ActorId,
        revision: peritus_types::RevisionTuple,
        resources: peritus_scheduler::ResourceVector,
        maximum_attempts: u16,
        limits: SchedulerLimits,
    ) -> Result<WorkSpec, EvaluationError> {
        let attempts = AttemptNumber::new(maximum_attempts).map_err(scheduling)?;
        WorkSpec::new(
            self.work_id,
            owner,
            revision,
            ExecutionClass::Coordination,
            128,
            resources,
            None,
            Vec::new(),
            None,
            attempts,
            RecoveryPolicy::Ambiguous,
            self.request_digest,
            limits,
        )
        .map_err(scheduling)
    }
}

#[allow(clippy::too_many_arguments, reason = "the digest deliberately binds every rollout field")]
fn request_digest(
    campaign: EvaluationCampaignId,
    profile: ProfileDigest,
    task: TaskId,
    partition: DatasetPartition,
    arm: EvaluationArm,
    ordinal: u16,
    seed: RolloutSeed,
    public_input: CandidateTaskInput,
    evaluator_binding: Sha256Digest,
    arm_binding: Sha256Digest,
    provider: Sha256Digest,
    model: Sha256Digest,
    execution: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = b"peritus.evaluation.rollout-request.v1\0".to_vec();
    bytes.extend_from_slice(campaign.as_bytes());
    bytes.extend_from_slice(profile.as_bytes());
    bytes.extend_from_slice(task.as_bytes());
    bytes.push(partition.tag());
    bytes.push(arm.tag());
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(seed.as_bytes());
    bytes.extend_from_slice(public_input.artifact().as_bytes());
    bytes.extend_from_slice(&public_input.byte_length().to_be_bytes());
    for digest in [evaluator_binding, arm_binding, provider, model, execution] {
        bytes.extend_from_slice(digest.as_bytes());
    }
    peritus_codec::sha256(&bytes)
}

fn schedule_key(
    profile: ProfileDigest,
    task: TaskId,
    ordinal: u16,
    arm: EvaluationArm,
    seed: RolloutSeed,
) -> SchedulingKey {
    let mut bytes = b"peritus.evaluation.schedule-block.v1\0".to_vec();
    bytes.extend_from_slice(profile.as_bytes());
    bytes.extend_from_slice(task.as_bytes());
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(seed.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let candidate_first = digest.as_bytes()[0] & 1 == 1;
    let within_pair = u8::from(match arm {
        EvaluationArm::Baseline => candidate_first,
        EvaluationArm::Candidate => !candidate_first,
    });
    SchedulingKey { block: *digest.as_bytes(), within_pair }
}

fn scheduling(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Scheduling,
        EvaluationOperation::Schedule,
        EvaluationRecovery::CorrectInput,
        "D3 rejected the exact rollout work specification",
    )
}
