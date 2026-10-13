//! Versioned complete recovery data. Decoded bindings remain inert until runtime admission.

use peritus_artifact_store::ArtifactDigest;
use peritus_harness::domain::{HarnessRevisionIdentity, RevisionDigest};
use peritus_scheduler::{ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};
use serde::{Deserialize, Serialize};

use crate::{
    CandidateTaskInput, DatasetId, DatasetManifest, DatasetPartition, DatasetTask, EvaluationArm,
    EvaluationError, EvaluationErrorKind, EvaluationExecutionMode, EvaluationLimits,
    EvaluationOperation, EvaluationRetryPolicy, ExecutionBinding, FrozenEvaluationProfile,
    FrozenModelControls, FrozenProviderSnapshot, HarnessArmBinding, InfrastructurePolicy,
    InfrastructureTreatment, MetricPolicy, SealedEvaluatorInput, SeedDeliveryPolicy, TaskId,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProfile {
    version: u16,
    digest: [u8; 32],
    dataset: StoredDataset,
    partitions: Vec<u8>,
    arms: [StoredArm; 2],
    provider: StoredProvider,
    model: StoredModel,
    execution: StoredExecution,
    retry: (u16, u64, u64),
    metrics: (Vec<u16>, u32, u32, u32, bool),
    infrastructure: [u8; 3],
    rollouts: u16,
    cross_lineage: bool,
    limits: (u32, u32, u16, u32, u16, u64),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDataset {
    id: [u8; 16],
    revision: u64,
    provenance: [u8; 32],
    tasks: Vec<StoredTask>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTask {
    id: [u8; 16],
    partition: u8,
    weight: u32,
    public: ([u8; 32], u64),
    sealed: ([u8; 32], u64, [u8; 32]),
    resources: [u8; 32],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredArm {
    ids: [[u8; 16]; 5],
    generation: u64,
    revision: u64,
    harness: ([u8; 16], u64, [u8; 32]),
    receipt: [u8; 32],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProvider {
    id: [u8; 16],
    revision: u64,
    digest: [u8; 32],
    sampling: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredModel {
    output: u64,
    temperature: Option<u32>,
    top_p: Option<u32>,
    template: [u8; 32],
    seed: u8,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredExecution {
    digests: [[u8; 32]; 4],
    resources: Vec<(u16, u64)>,
    deadline: u64,
    concurrency: u16,
    restricted: bool,
    teardown: bool,
    local_process: bool,
}

impl FrozenEvaluationProfile {
    /// Encodes complete versioned recovery input, including the disjoint dataset descriptors.
    ///
    /// # Errors
    /// Rejects serialization or a profile exceeding its frozen storage bound.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EvaluationError> {
        let e = self.execution();
        let m = self.model();
        let p = self.provider();
        let retry = self.retry();
        let metrics = self.metrics();
        let limits = self.limits();
        let infrastructure = self.infrastructure();
        let dataset = self.dataset();
        let record = StoredProfile {
            version: 1,
            digest: *self.digest().as_bytes(),
            dataset: StoredDataset {
                id: *dataset.id().as_bytes(),
                revision: dataset.revision(),
                provenance: *dataset.provenance_digest().as_bytes(),
                tasks: dataset
                    .tasks()
                    .iter()
                    .map(|t| StoredTask {
                        id: *t.id().as_bytes(),
                        partition: t.partition().tag(),
                        weight: t.weight(),
                        public: (
                            *t.candidate_input().artifact().as_bytes(),
                            t.candidate_input().byte_length(),
                        ),
                        sealed: (
                            *t.evaluator_input().artifact().as_bytes(),
                            t.evaluator_input().byte_length(),
                            *t.evaluator_input().verifier_digest().as_bytes(),
                        ),
                        resources: *t.resource_profile_digest().as_bytes(),
                    })
                    .collect(),
            },
            partitions: self.partitions().iter().map(|p| p.tag()).collect(),
            arms: [
                store_arm(self.arm(EvaluationArm::Baseline)),
                store_arm(self.arm(EvaluationArm::Candidate)),
            ],
            provider: StoredProvider {
                id: *p.profile_id().as_bytes(),
                revision: p.revision(),
                digest: *p.digest().as_bytes(),
                sampling: p.supports_sampling_controls(),
            },
            model: StoredModel {
                output: m.maximum_output_tokens(),
                temperature: m.temperature_millionths(),
                top_p: m.top_p_millionths(),
                template: *m.request_template_digest().as_bytes(),
                seed: m.seed_delivery().tag(),
            },
            execution: StoredExecution {
                digests: [
                    *e.sandbox_plan_digest().as_bytes(),
                    *e.backend_admission_digest().as_bytes(),
                    *e.environment_digest().as_bytes(),
                    *e.image_digest().as_bytes(),
                ],
                resources: e
                    .resource_request()
                    .entries()
                    .iter()
                    .map(|r| (r.kind().tag(), r.quantity().get()))
                    .collect(),
                deadline: e.deadline_micros(),
                concurrency: e.concurrency(),
                restricted: e.require_restricted_isolation(),
                teardown: e.require_complete_teardown(),
                local_process: e.mode() == EvaluationExecutionMode::LocalProcess,
            },
            retry: (
                retry.maximum_attempts(),
                retry.initial_backoff_micros(),
                retry.maximum_backoff_micros(),
            ),
            metrics: (
                metrics.pass_k().to_vec(),
                metrics.bootstrap_replicates(),
                metrics.confidence_millionths(),
                metrics.instability_threshold_millionths(),
                metrics.require_complete_usage(),
            ),
            infrastructure: [
                infrastructure.correctness().tag(),
                infrastructure.reliability().tag(),
                infrastructure.resource().tag(),
            ],
            rollouts: self.rollouts_per_task(),
            cross_lineage: self.cross_lineage(),
            limits: (
                limits.tasks(),
                limits.rollouts(),
                limits.attempts_per_rollout(),
                limits.bootstrap_replicates(),
                limits.pass_k_values(),
                limits.state_bytes(),
            ),
        };
        let bytes = serde_json::to_vec(&record).map_err(malformed)?;
        if bytes.len() as u64 > limits.state_bytes() {
            return Err(malformed("profile storage limit"));
        }
        Ok(bytes)
    }

    /// Restores every checked field and verifies the full profile digest and canonical encoding.
    /// This grants neither execution nor materialization authority.
    ///
    /// # Errors
    /// Rejects legacy Debug text, unknown versions, malformed fields, tampering or trailing data.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, EvaluationError> {
        if bytes.len() as u64 > EvaluationLimits::MAX_STATE_BYTES {
            return Err(malformed("profile storage limit"));
        }
        let s: StoredProfile = serde_json::from_slice(bytes).map_err(malformed)?;
        if s.version != 1 {
            return Err(malformed("unsupported profile version"));
        }
        let l = s.limits;
        let limits = EvaluationLimits::new(l.0, l.1, l.2, l.3, l.4, l.5)?;
        let mut tasks = Vec::with_capacity(s.dataset.tasks.len());
        if s.dataset.tasks.len() > limits.tasks() as usize {
            return Err(malformed("task storage limit"));
        }
        for t in s.dataset.tasks {
            tasks.push(DatasetTask::new(
                TaskId::new(t.id)?,
                partition(t.partition)?,
                t.weight,
                CandidateTaskInput::new(artifact(t.public.0), t.public.1)?,
                SealedEvaluatorInput::new(
                    artifact(t.sealed.0),
                    t.sealed.1,
                    Sha256Digest::new(t.sealed.2),
                )?,
                Sha256Digest::new(t.resources),
            )?);
        }
        let dataset = DatasetManifest::new(
            DatasetId::new(s.dataset.id)?,
            s.dataset.revision,
            tasks,
            Sha256Digest::new(s.dataset.provenance),
            limits,
        )?;
        let partitions = s.partitions.into_iter().map(partition).collect::<Result<Vec<_>, _>>()?;
        let [baseline, candidate] = s.arms;
        let provider = FrozenProviderSnapshot::restore(
            ProviderProfileId::new(s.provider.id).map_err(malformed)?,
            s.provider.revision,
            Sha256Digest::new(s.provider.digest),
            s.provider.sampling,
        )?;
        let seed = match s.model.seed {
            1 => SeedDeliveryPolicy::Required,
            2 => SeedDeliveryPolicy::RecordedOnly,
            _ => return Err(malformed("seed delivery tag")),
        };
        let model = FrozenModelControls::new(
            s.model.output,
            s.model.temperature,
            s.model.top_p,
            Sha256Digest::new(s.model.template),
            seed,
            provider,
        )?;
        let e = s.execution;
        let resources = e
            .resources
            .into_iter()
            .map(|(k, q)| {
                Ok(ResourceEntry::new(
                    ResourceKind::new(k).map_err(malformed)?,
                    ResourceQuantity::new(q).map_err(malformed)?,
                ))
            })
            .collect::<Result<Vec<_>, EvaluationError>>()?;
        let resources = ResourceVector::new(resources, u16::MAX).map_err(malformed)?;
        let mut execution = ExecutionBinding::new(
            Sha256Digest::new(e.digests[0]),
            Sha256Digest::new(e.digests[1]),
            Sha256Digest::new(e.digests[2]),
            Sha256Digest::new(e.digests[3]),
            resources,
            e.deadline,
            e.concurrency,
            e.restricted,
            e.teardown,
        )?;
        if e.local_process {
            execution = execution.with_local_process()?;
        }
        let retry = EvaluationRetryPolicy::new(s.retry.0, s.retry.1, s.retry.2, limits)?;
        let metrics = MetricPolicy::new(
            s.metrics.0,
            s.metrics.1,
            s.metrics.2,
            s.metrics.3,
            s.metrics.4,
            limits,
        )?;
        let infrastructure = InfrastructurePolicy::new(
            treatment(s.infrastructure[0])?,
            treatment(s.infrastructure[1])?,
            treatment(s.infrastructure[2])?,
        );
        let profile = Self::new(
            dataset,
            partitions,
            restore_arm(baseline)?,
            restore_arm(candidate)?,
            provider,
            model,
            execution,
            retry,
            metrics,
            infrastructure,
            s.rollouts,
            s.cross_lineage,
            limits,
        )?;
        if profile.digest().as_bytes() != &s.digest || profile.canonical_bytes()? != bytes {
            return Err(malformed("profile digest or canonical bytes differ"));
        }
        Ok(profile)
    }
}

fn store_arm(arm: HarnessArmBinding) -> StoredArm {
    let r = arm.revision();
    let h = arm.harness_revision();
    StoredArm {
        ids: [
            *r.acceptance_spec_id().as_bytes(),
            *r.harness_id().as_bytes(),
            *r.workspace_id().as_bytes(),
            *r.policy_id().as_bytes(),
            *r.provider_profile_id().as_bytes(),
        ],
        generation: r.workspace_generation().get(),
        revision: r.workspace_revision().get(),
        harness: (*h.harness_id().as_bytes(), h.number().get(), *h.digest().as_bytes()),
        receipt: *arm.receipt_digest().as_bytes(),
    }
}
fn restore_arm(s: StoredArm) -> Result<HarnessArmBinding, EvaluationError> {
    let r = RevisionTuple::new(
        AcceptanceSpecId::new(s.ids[0]).map_err(malformed)?,
        HarnessId::new(s.ids[1]).map_err(malformed)?,
        WorkspaceId::new(s.ids[2]).map_err(malformed)?,
        Generation::new(s.generation).map_err(malformed)?,
        RevisionNumber::new(s.revision).map_err(malformed)?,
        PolicyId::new(s.ids[3]).map_err(malformed)?,
        ProviderProfileId::new(s.ids[4]).map_err(malformed)?,
    );
    let h = HarnessRevisionIdentity::new(
        HarnessId::new(s.harness.0).map_err(malformed)?,
        RevisionNumber::new(s.harness.1).map_err(malformed)?,
        RevisionDigest::new(Sha256Digest::new(s.harness.2)),
    );
    Ok(HarnessArmBinding::new(r, h, Sha256Digest::new(s.receipt)))
}
fn artifact(bytes: [u8; 32]) -> ArtifactDigest {
    ArtifactDigest::from_sha256(Sha256Digest::new(bytes))
}
fn partition(tag: u8) -> Result<DatasetPartition, EvaluationError> {
    match tag {
        1 => Ok(DatasetPartition::Development),
        2 => Ok(DatasetPartition::Calibration),
        3 => Ok(DatasetPartition::Regression),
        4 => Ok(DatasetPartition::SealedHoldout),
        5 => Ok(DatasetPartition::Canary),
        _ => Err(malformed("partition tag")),
    }
}
fn treatment(tag: u8) -> Result<InfrastructureTreatment, EvaluationError> {
    match tag {
        1 => Ok(InfrastructureTreatment::CountAsFailure),
        2 => Ok(InfrastructureTreatment::ExcludeWithDenominator),
        3 => Ok(InfrastructureTreatment::InvalidateMetric),
        _ => Err(malformed("infrastructure tag")),
    }
}
fn malformed<T>(_: T) -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Profile,
        EvaluationOperation::FreezeProfile,
        "stored evaluation profile is unsupported, malformed, noncanonical or digest-mismatched",
    )
}
