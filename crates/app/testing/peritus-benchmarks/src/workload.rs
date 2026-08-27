//! Stable workload definitions for application-relevant load and soak scenarios.

use serde::{Deserialize, Serialize};

use crate::{QualificationError, ResourceEnvelope, StableId};

/// Application behavior selected by a workload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    /// Multiple complete run lifecycles overlap.
    ConcurrentRuns,
    /// Authoritative events are appended under sustained command intake.
    EventAppend,
    /// Output-heavy owned processes stream through bounded terminal delivery.
    TerminalStreaming,
    /// Active runs and processes are sampled for steady and peak memory.
    MemoryBounds,
    /// Owned process trees are repeatedly cancelled under output load.
    Cancellation,
    /// The subject repeatedly restarts and reconciles increasing journal sizes.
    Recovery,
    /// The authoritative and terminal queues are intentionally saturated.
    QueueSaturation,
    /// A deliberately slow telemetry exporter exercises spool backpressure.
    ExporterBackpressure,
    /// A deliberately rate-limited provider exercises provider backpressure.
    ProviderBackpressure,
    /// Token-heavy model streams exercise accounting and sustained throughput.
    TokenFlow,
    /// Large artifact streams exercise disk throughput, quotas, and collection pauses.
    DiskArtifacts,
}

/// Bounded parameters shared by deterministic load and soak plan generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct WorkloadParameters {
    duration_seconds: u64,
    operations_per_second: u32,
    operation_count: u64,
    max_concurrency: u32,
    payload_bytes: u32,
    memory_reservation_bytes: u64,
    disk_reservation_bytes: u64,
    token_reservation: u64,
    queue_capacity: u32,
    seed: u64,
}

impl WorkloadParameters {
    /// Constructs a finite deterministic schedule at the requested rate.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when duration, rate, or concurrency is zero or when their
    /// product overflows the operation-count representation.
    pub fn load(
        duration_seconds: u64,
        operations_per_second: u32,
        max_concurrency: u32,
    ) -> Result<Self, QualificationError> {
        if duration_seconds == 0 || operations_per_second == 0 || max_concurrency == 0 {
            return Err(QualificationError::invalid_value(
                "workload.schedule",
                "duration, rate, and concurrency must be greater than zero",
            ));
        }
        let operation_count = duration_seconds
            .checked_mul(u64::from(operations_per_second))
            .ok_or(QualificationError::ArithmeticOverflow("workload operation count"))?;
        Ok(Self {
            duration_seconds,
            operations_per_second,
            operation_count,
            max_concurrency,
            payload_bytes: 4096,
            memory_reservation_bytes: 1,
            disk_reservation_bytes: 1,
            token_reservation: 1,
            queue_capacity: max_concurrency,
            seed: 0,
        })
    }

    /// Sets the stable seed used by lazy operation generation.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the per-operation payload size.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the payload size is zero.
    pub const fn with_payload_bytes(
        mut self,
        payload_bytes: u32,
    ) -> Result<Self, QualificationError> {
        if payload_bytes == 0 {
            return Err(QualificationError::invalid_value(
                "workload.payload_bytes",
                "must be greater than zero",
            ));
        }
        self.payload_bytes = payload_bytes;
        Ok(self)
    }

    /// Sets per-slot memory, disk, and token reservations.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when any reservation is zero.
    pub const fn with_reservations(
        mut self,
        memory_bytes: u64,
        disk_bytes: u64,
        tokens: u64,
    ) -> Result<Self, QualificationError> {
        if memory_bytes == 0 || disk_bytes == 0 || tokens == 0 {
            return Err(QualificationError::invalid_value(
                "workload.reservations",
                "all reservations must be greater than zero",
            ));
        }
        self.memory_reservation_bytes = memory_bytes;
        self.disk_reservation_bytes = disk_bytes;
        self.token_reservation = tokens;
        Ok(self)
    }

    /// Sets the queue capacity exercised by saturation scenarios.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the queue capacity is zero.
    pub const fn with_queue_capacity(
        mut self,
        queue_capacity: u32,
    ) -> Result<Self, QualificationError> {
        if queue_capacity == 0 {
            return Err(QualificationError::invalid_value(
                "workload.queue_capacity",
                "must be greater than zero",
            ));
        }
        self.queue_capacity = queue_capacity;
        Ok(self)
    }

    /// Returns planned duration in seconds.
    #[must_use]
    pub const fn duration_seconds(self) -> u64 {
        self.duration_seconds
    }

    /// Returns planned operations per second.
    #[must_use]
    pub const fn operations_per_second(self) -> u32 {
        self.operations_per_second
    }

    /// Returns total lazily generated operations.
    #[must_use]
    pub const fn operation_count(self) -> u64 {
        self.operation_count
    }

    /// Returns maximum subject concurrency.
    #[must_use]
    pub const fn max_concurrency(self) -> u32 {
        self.max_concurrency
    }

    /// Returns operation payload bytes.
    #[must_use]
    pub const fn payload_bytes(self) -> u32 {
        self.payload_bytes
    }

    /// Returns per-slot memory reservation bytes.
    #[must_use]
    pub const fn memory_reservation_bytes(self) -> u64 {
        self.memory_reservation_bytes
    }

    /// Returns per-slot disk reservation bytes.
    #[must_use]
    pub const fn disk_reservation_bytes(self) -> u64 {
        self.disk_reservation_bytes
    }

    /// Returns per-slot token reservation.
    #[must_use]
    pub const fn token_reservation(self) -> u64 {
        self.token_reservation
    }

    /// Returns the exercised queue capacity.
    #[must_use]
    pub const fn queue_capacity(self) -> u32 {
        self.queue_capacity
    }

    /// Returns the stable deterministic seed.
    #[must_use]
    pub const fn seed(self) -> u64 {
        self.seed
    }
}

/// A validated stable workload dataset entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Workload {
    id: StableId,
    description: String,
    scenario: ScenarioKind,
    parameters: WorkloadParameters,
}

impl Workload {
    /// Constructs a workload and validates its descriptive text.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the description is empty or exceeds 512 bytes.
    pub fn new(
        id: StableId,
        description: impl Into<String>,
        scenario: ScenarioKind,
        parameters: WorkloadParameters,
    ) -> Result<Self, QualificationError> {
        let description = description.into();
        if description.trim().is_empty() || description.len() > 512 {
            return Err(QualificationError::invalid_value(
                "workload.description",
                "must contain 1 through 512 bytes",
            ));
        }
        Ok(Self { id, description, scenario, parameters })
    }

    /// Validates that declared workload reservations fit a qualification profile.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when concurrency, memory, disk, token, or queue reservations
    /// exceed the selected profile envelope.
    pub fn validate_against(&self, envelope: ResourceEnvelope) -> Result<(), QualificationError> {
        let concurrency = u64::from(self.parameters.max_concurrency());
        let profile_concurrency = envelope.concurrency();
        let concurrent_limit = match self.scenario {
            ScenarioKind::TerminalStreaming | ScenarioKind::Cancellation => {
                profile_concurrency.processes()
            }
            ScenarioKind::ProviderBackpressure | ScenarioKind::TokenFlow => {
                profile_concurrency.provider_requests()
            }
            ScenarioKind::ConcurrentRuns
            | ScenarioKind::EventAppend
            | ScenarioKind::MemoryBounds
            | ScenarioKind::Recovery
            | ScenarioKind::QueueSaturation
            | ScenarioKind::ExporterBackpressure
            | ScenarioKind::DiskArtifacts => profile_concurrency.runs(),
        };
        self.require(self.parameters.max_concurrency() <= concurrent_limit, "concurrency")?;
        let capacities = envelope.capacity();
        self.require(
            self.parameters.memory_reservation_bytes().saturating_mul(concurrency)
                <= capacities.memory_bytes(),
            "memory",
        )?;
        self.require(
            self.parameters.disk_reservation_bytes().saturating_mul(concurrency)
                <= capacities.disk_bytes(),
            "disk",
        )?;
        self.require(
            self.parameters.token_reservation().saturating_mul(concurrency) <= capacities.tokens(),
            "tokens",
        )?;
        let queue_limit = match self.scenario {
            ScenarioKind::TerminalStreaming | ScenarioKind::Cancellation => {
                envelope.queues().terminal()
            }
            ScenarioKind::ExporterBackpressure => envelope.queues().exporter(),
            ScenarioKind::ProviderBackpressure | ScenarioKind::TokenFlow => {
                envelope.queues().provider()
            }
            ScenarioKind::ConcurrentRuns
            | ScenarioKind::EventAppend
            | ScenarioKind::MemoryBounds
            | ScenarioKind::Recovery
            | ScenarioKind::QueueSaturation
            | ScenarioKind::DiskArtifacts => envelope.queues().command(),
        };
        self.require(self.parameters.queue_capacity() <= queue_limit, "queue")
    }

    fn require(&self, condition: bool, resource: &'static str) -> Result<(), QualificationError> {
        if condition {
            Ok(())
        } else {
            Err(QualificationError::WorkloadExceedsProfile {
                workload: self.id.to_string(),
                resource,
            })
        }
    }

    /// Returns the stable workload identifier.
    #[must_use]
    pub const fn id(&self) -> &StableId {
        &self.id
    }

    /// Returns the workload purpose.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the selected application scenario.
    #[must_use]
    pub const fn scenario(&self) -> ScenarioKind {
        self.scenario
    }

    /// Returns bounded plan parameters.
    #[must_use]
    pub const fn parameters(&self) -> WorkloadParameters {
        self.parameters
    }
}
