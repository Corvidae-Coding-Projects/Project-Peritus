//! Complete immutable campaign profile and arm/execution bindings.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_harness::{GoverningHarnessBinding, domain::HarnessRevisionIdentity};
use peritus_scheduler::ResourceVector;
use peritus_types::{RevisionTuple, Sha256Digest};

use crate::{
    DatasetManifest, DatasetPartition, EvaluationError, EvaluationErrorKind, EvaluationLimits,
    EvaluationOperation, EvaluationRecovery, EvaluationRetryPolicy, FrozenModelControls,
    FrozenProviderSnapshot, InfrastructurePolicy, MetricPolicy, ProfileDigest,
};

const ARM_DOMAIN: &[u8] = b"peritus.evaluation.harness-arm.v1\0";
const EXECUTION_DOMAIN: &[u8] = b"peritus.evaluation.execution-binding.v1\0";
const PROFILE_DOMAIN: &[u8] = b"peritus.evaluation.profile.v1\0";

/// Closed baseline/candidate comparison arm.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvaluationArm {
    /// Existing harness used as the control.
    Baseline,
    /// Candidate harness under evaluation.
    Candidate,
}

impl EvaluationArm {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Baseline => 1,
            Self::Candidate => 2,
        }
    }
}

/// Exact E1 materialized harness binding for one evaluation arm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HarnessArmBinding {
    revision: RevisionTuple,
    harness_revision: HarnessRevisionIdentity,
    receipt_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl HarnessArmBinding {
    /// Reconstructs a frozen arm from exact E1 identities and a materialization receipt digest.
    ///
    /// The caller must have resolved `harness_revision` through E1; this inert value does not
    /// grant materialization authority.
    #[must_use]
    pub fn new(
        revision: RevisionTuple,
        harness_revision: HarnessRevisionIdentity,
        receipt_digest: Sha256Digest,
    ) -> Self {
        let mut bytes = Vec::with_capacity(160);
        bytes.extend_from_slice(ARM_DOMAIN);
        bytes.extend_from_slice(peritus_evidence::revision_digest(&revision).as_bytes());
        bytes.extend_from_slice(harness_revision.harness_id().as_bytes());
        bytes.extend_from_slice(&harness_revision.number().get().to_be_bytes());
        bytes.extend_from_slice(harness_revision.digest().as_bytes());
        bytes.extend_from_slice(receipt_digest.as_bytes());
        let digest = peritus_codec::sha256(&bytes);
        Self { revision, harness_revision, receipt_digest, digest }
    }

    /// Captures an already checked E1 governing binding.
    #[must_use]
    pub fn capture(binding: &GoverningHarnessBinding) -> Self {
        Self::new(
            binding.revision(),
            binding.harness_revision(),
            binding.materialization().digest(),
        )
    }
    /// Shared authority/evidence revision tuple.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    /// Exact E1 branch-distinguishing revision.
    #[must_use]
    pub const fn harness_revision(self) -> HarnessRevisionIdentity {
        self.harness_revision
    }
    /// Exact E1 materialization receipt digest.
    #[must_use]
    pub const fn receipt_digest(self) -> Sha256Digest {
        self.receipt_digest
    }
    /// Complete arm-binding digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

/// Frozen runtime-neutral C2/C3 execution requirements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionBinding {
    sandbox_plan_digest: Sha256Digest,
    backend_admission_digest: Sha256Digest,
    environment_digest: Sha256Digest,
    image_digest: Sha256Digest,
    resource_request: ResourceVector,
    deadline_micros: u64,
    concurrency: u16,
    require_restricted_isolation: bool,
    require_complete_teardown: bool,
    digest: Sha256Digest,
}

impl ExecutionBinding {
    /// Creates complete checked execution requirements.
    ///
    /// # Errors
    /// Rejects zero deadline/concurrency.
    #[allow(
        clippy::too_many_arguments,
        reason = "the immutable C2/C3 binding is intentionally complete"
    )]
    pub fn new(
        sandbox_plan_digest: Sha256Digest,
        backend_admission_digest: Sha256Digest,
        environment_digest: Sha256Digest,
        image_digest: Sha256Digest,
        resource_request: ResourceVector,
        deadline_micros: u64,
        concurrency: u16,
        require_restricted_isolation: bool,
        require_complete_teardown: bool,
    ) -> Result<Self, EvaluationError> {
        if deadline_micros == 0 || concurrency == 0 {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "execution deadline or concurrency is zero",
            ));
        }
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(EXECUTION_DOMAIN).map_err(codec)?;
        for digest in
            [sandbox_plan_digest, backend_admission_digest, environment_digest, image_digest]
        {
            writer.write_fixed(digest.as_bytes()).map_err(codec)?;
        }
        writer.write_collection_len(resource_request.entries().len()).map_err(codec)?;
        for entry in resource_request.entries() {
            writer.write_u16(entry.kind().tag()).map_err(codec)?;
            writer.write_u64(entry.quantity().get()).map_err(codec)?;
        }
        writer.write_u64(deadline_micros).map_err(codec)?;
        writer.write_u16(concurrency).map_err(codec)?;
        writer.write_bool(require_restricted_isolation).map_err(codec)?;
        writer.write_bool(require_complete_teardown).map_err(codec)?;
        let digest = peritus_codec::sha256(&writer.into_bytes());
        Ok(Self {
            sandbox_plan_digest,
            backend_admission_digest,
            environment_digest,
            image_digest,
            resource_request,
            deadline_micros,
            concurrency,
            require_restricted_isolation,
            require_complete_teardown,
            digest,
        })
    }
    /// Sandbox-plan digest.
    #[must_use]
    pub const fn sandbox_plan_digest(&self) -> Sha256Digest {
        self.sandbox_plan_digest
    }
    /// Backend-admission digest.
    #[must_use]
    pub const fn backend_admission_digest(&self) -> Sha256Digest {
        self.backend_admission_digest
    }
    /// Environment digest.
    #[must_use]
    pub const fn environment_digest(&self) -> Sha256Digest {
        self.environment_digest
    }
    /// Sandbox image digest.
    #[must_use]
    pub const fn image_digest(&self) -> Sha256Digest {
        self.image_digest
    }
    /// Requested D3 resources.
    #[must_use]
    pub const fn resource_request(&self) -> &ResourceVector {
        &self.resource_request
    }
    /// Wall deadline in microseconds.
    #[must_use]
    pub const fn deadline_micros(&self) -> u64 {
        self.deadline_micros
    }
    /// Maximum simultaneous rollouts.
    #[must_use]
    pub const fn concurrency(&self) -> u16 {
        self.concurrency
    }
    /// Whether restricted isolation is mandatory.
    #[must_use]
    pub const fn require_restricted_isolation(&self) -> bool {
        self.require_restricted_isolation
    }
    /// Whether complete process-tree teardown is mandatory.
    #[must_use]
    pub const fn require_complete_teardown(&self) -> bool {
        self.require_complete_teardown
    }
    /// Complete execution-binding digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Complete checked immutable E3 evaluation profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenEvaluationProfile {
    dataset: DatasetManifest,
    partitions: Vec<DatasetPartition>,
    baseline: HarnessArmBinding,
    candidate: HarnessArmBinding,
    provider: FrozenProviderSnapshot,
    model: FrozenModelControls,
    execution: ExecutionBinding,
    retry: EvaluationRetryPolicy,
    metrics: MetricPolicy,
    infrastructure: InfrastructurePolicy,
    rollouts_per_task: u16,
    cross_lineage: bool,
    limits: EvaluationLimits,
    digest: ProfileDigest,
}

impl FrozenEvaluationProfile {
    /// Validates and freezes every campaign input.
    ///
    /// # Errors
    /// Rejects noncanonical partitions, equal arms, undeclared work, or bounded-size overflow.
    #[allow(
        clippy::too_many_arguments,
        reason = "the complete frozen campaign profile stays explicit"
    )]
    pub fn new(
        dataset: DatasetManifest,
        partitions: Vec<DatasetPartition>,
        baseline: HarnessArmBinding,
        candidate: HarnessArmBinding,
        provider: FrozenProviderSnapshot,
        model: FrozenModelControls,
        execution: ExecutionBinding,
        retry: EvaluationRetryPolicy,
        metrics: MetricPolicy,
        infrastructure: InfrastructurePolicy,
        rollouts_per_task: u16,
        cross_lineage: bool,
        limits: EvaluationLimits,
    ) -> Result<Self, EvaluationError> {
        if partitions.is_empty()
            || partitions.windows(2).any(|pair| pair[0] >= pair[1])
            || rollouts_per_task == 0
            || baseline.digest() == candidate.digest()
            || !cross_lineage
                && baseline.harness_revision().harness_id()
                    != candidate.harness_revision().harness_id()
            || !dataset.tasks().iter().any(|task| partitions.contains(&task.partition()))
        {
            return Err(crate::invalid(
                EvaluationErrorKind::Profile,
                EvaluationOperation::FreezeProfile,
                "profile partitions or harness arms are invalid",
            ));
        }
        let selected =
            dataset.tasks().iter().filter(|task| partitions.contains(&task.partition())).count();
        let rollout_count = selected
            .checked_mul(2)
            .and_then(|value| value.checked_mul(usize::from(rollouts_per_task)))
            .ok_or_else(|| {
                crate::invalid(
                    EvaluationErrorKind::LimitExceeded,
                    EvaluationOperation::FreezeProfile,
                    "profile rollout cardinality overflowed",
                )
            })?;
        if rollout_count > usize::try_from(limits.rollouts()).unwrap_or(usize::MAX) {
            return Err(EvaluationError::new(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::FreezeProfile,
                EvaluationRecovery::ReduceScope,
                "profile exceeds the logical rollout ceiling",
            ));
        }
        let digest = ProfileDigest::new(profile_digest(
            &dataset,
            &partitions,
            baseline,
            candidate,
            provider,
            model,
            &execution,
            retry,
            &metrics,
            infrastructure,
            rollouts_per_task,
            cross_lineage,
            limits,
        )?);
        Ok(Self {
            dataset,
            partitions,
            baseline,
            candidate,
            provider,
            model,
            execution,
            retry,
            metrics,
            infrastructure,
            rollouts_per_task,
            cross_lineage,
            limits,
            digest,
        })
    }

    /// Borrows the exact checked dataset.
    #[must_use]
    pub const fn dataset(&self) -> &DatasetManifest {
        &self.dataset
    }
    /// Borrows selected canonical partitions.
    #[must_use]
    pub fn partitions(&self) -> &[DatasetPartition] {
        &self.partitions
    }
    /// Returns one exact arm binding.
    #[must_use]
    pub const fn arm(&self, arm: EvaluationArm) -> HarnessArmBinding {
        match arm {
            EvaluationArm::Baseline => self.baseline,
            EvaluationArm::Candidate => self.candidate,
        }
    }
    /// Returns the C5 snapshot.
    #[must_use]
    pub const fn provider(&self) -> FrozenProviderSnapshot {
        self.provider
    }
    /// Returns model controls.
    #[must_use]
    pub const fn model(&self) -> FrozenModelControls {
        self.model
    }
    /// Borrows execution requirements.
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    /// Returns retry policy.
    #[must_use]
    pub const fn retry(&self) -> EvaluationRetryPolicy {
        self.retry
    }
    /// Borrows metric policy.
    #[must_use]
    pub const fn metrics(&self) -> &MetricPolicy {
        &self.metrics
    }
    /// Returns infrastructure policy.
    #[must_use]
    pub const fn infrastructure(&self) -> InfrastructurePolicy {
        self.infrastructure
    }
    /// Returns rollouts per selected task and arm.
    #[must_use]
    pub const fn rollouts_per_task(&self) -> u16 {
        self.rollouts_per_task
    }
    /// Whether arm lineages may differ.
    #[must_use]
    pub const fn cross_lineage(&self) -> bool {
        self.cross_lineage
    }
    /// Returns enforced limits.
    #[must_use]
    pub const fn limits(&self) -> EvaluationLimits {
        self.limits
    }
    /// Returns the complete profile digest.
    #[must_use]
    pub const fn digest(&self) -> ProfileDigest {
        self.digest
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "canonical profile identity intentionally covers every field"
)]
fn profile_digest(
    dataset: &DatasetManifest,
    partitions: &[DatasetPartition],
    baseline: HarnessArmBinding,
    candidate: HarnessArmBinding,
    provider: FrozenProviderSnapshot,
    model: FrozenModelControls,
    execution: &ExecutionBinding,
    retry: EvaluationRetryPolicy,
    metrics: &MetricPolicy,
    infrastructure: InfrastructurePolicy,
    rollouts: u16,
    cross_lineage: bool,
    limits: EvaluationLimits,
) -> Result<Sha256Digest, EvaluationError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(PROFILE_DOMAIN).map_err(codec)?;
    writer.write_fixed(dataset.digest().as_bytes()).map_err(codec)?;
    writer.write_collection_len(partitions.len()).map_err(codec)?;
    for partition in partitions {
        writer.write_u8(partition.tag()).map_err(codec)?;
    }
    for digest in [
        baseline.digest(),
        candidate.digest(),
        provider.digest(),
        model.digest(),
        execution.digest(),
    ] {
        writer.write_fixed(digest.as_bytes()).map_err(codec)?;
    }
    writer.write_u16(retry.maximum_attempts()).map_err(codec)?;
    writer.write_u64(retry.initial_backoff_micros()).map_err(codec)?;
    writer.write_u64(retry.maximum_backoff_micros()).map_err(codec)?;
    writer.write_collection_len(metrics.pass_k().len()).map_err(codec)?;
    for value in metrics.pass_k() {
        writer.write_u16(*value).map_err(codec)?;
    }
    writer.write_u32(metrics.bootstrap_replicates()).map_err(codec)?;
    writer.write_u32(metrics.confidence_millionths()).map_err(codec)?;
    writer.write_u32(metrics.instability_threshold_millionths()).map_err(codec)?;
    writer.write_bool(metrics.require_complete_usage()).map_err(codec)?;
    writer.write_u8(infrastructure.correctness().tag()).map_err(codec)?;
    writer.write_u8(infrastructure.reliability().tag()).map_err(codec)?;
    writer.write_u8(infrastructure.resource().tag()).map_err(codec)?;
    writer.write_u16(rollouts).map_err(codec)?;
    writer.write_bool(cross_lineage).map_err(codec)?;
    writer.write_u32(limits.tasks()).map_err(codec)?;
    writer.write_u32(limits.rollouts()).map_err(codec)?;
    writer.write_u16(limits.attempts_per_rollout()).map_err(codec)?;
    writer.write_u32(limits.bootstrap_replicates()).map_err(codec)?;
    writer.write_u16(limits.pass_k_values()).map_err(codec)?;
    writer.write_u64(limits.state_bytes()).map_err(codec)?;
    Ok(peritus_codec::sha256(&writer.into_bytes()))
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::FreezeProfile,
        EvaluationRecovery::ReduceScope,
        "frozen profile exceeds production codec limits",
    )
}
