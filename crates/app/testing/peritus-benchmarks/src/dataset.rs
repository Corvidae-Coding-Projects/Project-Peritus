//! Bounded JSON loading for stable profiles, workloads, and accepted baselines.

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::{
    CapacityLimits, ConcurrencyLimits, Metric, ObjectiveBound, QualificationError,
    QualificationProfile, QualificationProfileBuilder, QueueLimits, ReferenceMachine,
    RegressionPolicy, ResourceEnvelope, ScenarioKind, SloObjective, StableId, Statistic, Workload,
    WorkloadParameters,
};

/// Byte and entry limits applied before a stable dataset is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatasetLimits {
    profile_bytes: usize,
    workload_bytes: usize,
    baseline_bytes: usize,
    max_workloads: usize,
    max_objectives: usize,
}

impl DatasetLimits {
    /// Constructs explicit nonzero document and collection limits.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when any byte or entry limit is zero.
    pub fn new(
        profile_bytes: usize,
        workload_bytes: usize,
        baseline_bytes: usize,
        max_workloads: usize,
        max_objectives: usize,
    ) -> Result<Self, QualificationError> {
        if [profile_bytes, workload_bytes, baseline_bytes, max_workloads, max_objectives]
            .contains(&0)
        {
            return Err(QualificationError::invalid_value(
                "dataset_limits",
                "all limits must be greater than zero",
            ));
        }
        Ok(Self { profile_bytes, workload_bytes, baseline_bytes, max_workloads, max_objectives })
    }

    /// Returns conservative limits for checked-in qualification datasets.
    #[must_use]
    pub const fn production_defaults() -> Self {
        Self {
            profile_bytes: 256 * 1024,
            workload_bytes: 512 * 1024,
            baseline_bytes: 512 * 1024,
            max_workloads: 128,
            max_objectives: 512,
        }
    }

    pub(crate) const fn baseline_bytes(self) -> usize {
        self.baseline_bytes
    }

    pub(crate) const fn max_objectives(self) -> usize {
        self.max_objectives
    }
}

/// Profile and workload catalog validated as one qualification dataset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationDataset {
    profile: QualificationProfile,
    workloads: Vec<Workload>,
}

impl QualificationDataset {
    /// Decodes bounded JSON documents and validates every cross-reference and resource reservation.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when either document exceeds its limit, JSON or schema
    /// validation fails, keys are duplicated, references dangle, or a workload exceeds the profile.
    pub fn from_json(
        profile_document: &str,
        workload_document: &str,
        limits: DatasetLimits,
    ) -> Result<Self, QualificationError> {
        require_document_limit("profile", profile_document, limits.profile_bytes)?;
        require_document_limit("workload", workload_document, limits.workload_bytes)?;
        let raw_profile: ProfileWire = serde_json::from_str(profile_document)
            .map_err(|source| QualificationError::Json { kind: "profile", source })?;
        let raw_workloads: WorkloadCatalogWire = serde_json::from_str(workload_document)
            .map_err(|source| QualificationError::Json { kind: "workload", source })?;
        if raw_profile.objectives.len() > limits.max_objectives
            || raw_workloads.workloads.len() > limits.max_workloads
        {
            return Err(QualificationError::invalid_value(
                "dataset.entries",
                "entry count exceeds configured dataset limits",
            ));
        }
        let profile = raw_profile.validate()?;
        let mut workloads = raw_workloads.validate()?;
        workloads.sort_by(|left, right| left.id().cmp(right.id()));
        for pair in workloads.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(QualificationError::Duplicate {
                    kind: "workload",
                    id: pair[0].id().to_string(),
                });
            }
        }
        let ids = workloads.iter().map(Workload::id).collect::<BTreeSet<_>>();
        for required in profile.required_workloads() {
            if !ids.contains(required) {
                return Err(QualificationError::UnknownReference {
                    kind: "required workload",
                    id: required.to_string(),
                });
            }
        }
        for workload in &workloads {
            workload.validate_against(profile.envelope())?;
        }
        Ok(Self { profile, workloads })
    }

    /// Returns the validated qualification profile.
    #[must_use]
    pub const fn profile(&self) -> &QualificationProfile {
        &self.profile
    }

    /// Returns workloads in stable identifier order.
    #[must_use]
    pub fn workloads(&self) -> &[Workload] {
        &self.workloads
    }

    /// Finds one workload by stable identifier.
    #[must_use]
    pub fn workload(&self, id: &StableId) -> Option<&Workload> {
        self.workloads.iter().find(|workload| workload.id() == id)
    }
}

const fn require_document_limit(
    kind: &'static str,
    document: &str,
    limit: usize,
) -> Result<(), QualificationError> {
    if document.len() <= limit {
        Ok(())
    } else {
        Err(QualificationError::DocumentLimit { kind, limit })
    }
}

const fn require_schema(
    schema_version: u32,
    field: &'static str,
) -> Result<(), QualificationError> {
    if schema_version == 1 {
        Ok(())
    } else {
        Err(QualificationError::invalid_value(field, "only schema version 1 is supported"))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileWire {
    schema_version: u32,
    id: String,
    description: String,
    reference_machine: ReferenceMachineWire,
    resource_envelope: ResourceEnvelopeWire,
    regression_policy: RegressionPolicyWire,
    max_measurements: usize,
    required_workloads: Vec<String>,
    objectives: Vec<ObjectiveWire>,
}

impl ProfileWire {
    fn validate(self) -> Result<QualificationProfile, QualificationError> {
        require_schema(self.schema_version, "profile.schema_version")?;
        let machine = self.reference_machine.validate()?;
        let envelope = self.resource_envelope.validate()?;
        let policy = self.regression_policy.validate()?;
        let mut builder = QualificationProfileBuilder::new(
            StableId::new(self.id)?,
            self.description,
            machine,
            envelope,
            policy,
        )
        .max_measurements(self.max_measurements)?;
        for required in self.required_workloads {
            builder = builder.required_workload(StableId::new(required)?);
        }
        for objective in self.objectives {
            builder = builder.objective(objective.validate()?);
        }
        builder.build()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceMachineWire {
    operating_system: String,
    architecture: String,
    cpu_model: String,
    logical_cores: u16,
    memory_bytes: u64,
    storage_class: String,
}

impl ReferenceMachineWire {
    fn validate(self) -> Result<ReferenceMachine, QualificationError> {
        ReferenceMachine::new(
            StableId::new(self.operating_system)?,
            StableId::new(self.architecture)?,
            self.cpu_model,
            self.logical_cores,
            self.memory_bytes,
            StableId::new(self.storage_class)?,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceEnvelopeWire {
    max_active_runs: u32,
    max_active_processes: u32,
    max_provider_requests: u32,
    max_memory_bytes: u64,
    max_disk_bytes: u64,
    max_tokens: u64,
    command_queue_capacity: u32,
    terminal_queue_capacity: u32,
    exporter_queue_capacity: u32,
    provider_queue_capacity: u32,
}

impl ResourceEnvelopeWire {
    fn validate(self) -> Result<ResourceEnvelope, QualificationError> {
        Ok(ResourceEnvelope::new(
            ConcurrencyLimits::new(
                self.max_active_runs,
                self.max_active_processes,
                self.max_provider_requests,
            )?,
            CapacityLimits::new(self.max_memory_bytes, self.max_disk_bytes, self.max_tokens)?,
            QueueLimits::new(
                self.command_queue_capacity,
                self.terminal_queue_capacity,
                self.exporter_queue_capacity,
                self.provider_queue_capacity,
            )?,
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegressionPolicyWire {
    warning_basis_points: u16,
    blocking_basis_points: u16,
    minimum_absolute_delta: u64,
    baseline_required: bool,
}

impl RegressionPolicyWire {
    const fn validate(self) -> Result<RegressionPolicy, QualificationError> {
        RegressionPolicy::new(
            self.warning_basis_points,
            self.blocking_basis_points,
            self.minimum_absolute_delta,
            self.baseline_required,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectiveWire {
    id: String,
    workload_id: String,
    metric: Metric,
    statistic: Statistic,
    bound: ObjectiveBound,
    threshold: u64,
    minimum_samples: usize,
}

impl ObjectiveWire {
    fn validate(self) -> Result<SloObjective, QualificationError> {
        SloObjective::new(
            StableId::new(self.id)?,
            StableId::new(self.workload_id)?,
            self.metric,
            self.statistic,
            self.bound,
            self.threshold,
            self.minimum_samples,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkloadCatalogWire {
    schema_version: u32,
    workloads: Vec<WorkloadWire>,
}

impl WorkloadCatalogWire {
    fn validate(self) -> Result<Vec<Workload>, QualificationError> {
        require_schema(self.schema_version, "workloads.schema_version")?;
        self.workloads.into_iter().map(WorkloadWire::validate).collect()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkloadWire {
    id: String,
    description: String,
    scenario: ScenarioKind,
    duration_seconds: u64,
    operations_per_second: u32,
    max_concurrency: u32,
    payload_bytes: u32,
    memory_reservation_bytes: u64,
    disk_reservation_bytes: u64,
    token_reservation: u64,
    queue_capacity: u32,
    seed: u64,
}

impl WorkloadWire {
    fn validate(self) -> Result<Workload, QualificationError> {
        let parameters = WorkloadParameters::load(
            self.duration_seconds,
            self.operations_per_second,
            self.max_concurrency,
        )?
        .with_seed(self.seed)
        .with_payload_bytes(self.payload_bytes)?
        .with_reservations(
            self.memory_reservation_bytes,
            self.disk_reservation_bytes,
            self.token_reservation,
        )?
        .with_queue_capacity(self.queue_capacity)?;
        Workload::new(StableId::new(self.id)?, self.description, self.scenario, parameters)
    }
}
