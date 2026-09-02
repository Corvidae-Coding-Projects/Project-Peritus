//! Lazy deterministic load and long-horizon soak schedules.

use std::cmp::Ordering;

use serde::Serialize;

use crate::plan_iterator::PlanIter;
use crate::{QualificationError, QueueKind, ResourceEnvelope, ScenarioKind, StableId, Workload};

/// Qualification schedule category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    /// A finite high-pressure schedule intended for an interactive qualification window.
    Load,
    /// A long-running schedule intended to expose leaks, drift, and recovery accumulation.
    Soak,
}

/// One effect request generated for a qualification subject.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum PlannedOperation {
    /// Starts one run under explicit resource reservations.
    StartRun {
        /// Stable plan-local run number.
        run: u64,
        /// Reserved resident bytes.
        memory_bytes: u64,
        /// Reserved disk bytes.
        disk_bytes: u64,
        /// Reserved tokens.
        tokens: u64,
    },
    /// Finishes and releases one plan-local run.
    FinishRun {
        /// Stable plan-local run number.
        run: u64,
    },
    /// Starts one owned process.
    StartProcess {
        /// Stable plan-local process number.
        process: u64,
        /// Reserved resident bytes.
        memory_bytes: u64,
    },
    /// Streams a bounded terminal output chunk.
    StreamTerminal {
        /// Stable plan-local process number.
        process: u64,
        /// Requested chunk length.
        bytes: u32,
    },
    /// Requests cancellation of an owned process tree.
    CancelProcess {
        /// Stable plan-local process number.
        process: u64,
    },
    /// Observes terminal completion and releases process resources.
    FinishProcess {
        /// Stable plan-local process number.
        process: u64,
    },
    /// Appends one authoritative event with a bounded payload.
    AppendEvent {
        /// Requested encoded payload length.
        bytes: u32,
    },
    /// Crashes the disposable qualification subject at a known journal size.
    CrashDaemon {
        /// Number of journal events expected before the crash.
        journal_events: u64,
    },
    /// Restarts and reconciles the disposable qualification subject.
    RestartDaemon,
    /// Adds items to a bounded queue.
    Enqueue {
        /// Queue selected by the scenario.
        queue: QueueKind,
        /// Number of items to add.
        count: u32,
    },
    /// Removes items from a bounded queue.
    Dequeue {
        /// Queue selected by the scenario.
        queue: QueueKind,
        /// Number of items to remove.
        count: u32,
    },
    /// Releases every queue item retained by a final partial saturation cycle.
    DrainQueue {
        /// Queue being returned to its terminal empty state.
        queue: QueueKind,
        /// Exact number of items retained before the final plan step; zero is permitted.
        count: u32,
    },
    /// Measures the producer-visible wait while a queue is full.
    ObserveBackpressure {
        /// Saturated queue.
        queue: QueueKind,
    },
    /// Starts one provider request.
    StartProviderRequest {
        /// Stable plan-local request number.
        request: u64,
    },
    /// Accounts a bounded token chunk for a provider request.
    AccountTokens {
        /// Stable plan-local request number.
        request: u64,
        /// Token chunk length.
        tokens: u64,
    },
    /// Finishes one provider request.
    FinishProviderRequest {
        /// Stable plan-local request number.
        request: u64,
    },
    /// Streams an artifact chunk to durable storage.
    WriteArtifact {
        /// Requested chunk length.
        bytes: u32,
    },
    /// Runs bounded artifact garbage collection.
    CollectArtifacts,
    /// Samples resident memory and retained resource gauges.
    SampleResources,
}

/// One scheduled operation with stable sequence and logical time.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanStep {
    sequence: u64,
    offset_micros: u64,
    operation: PlannedOperation,
}

impl PlanStep {
    /// Returns the zero-based deterministic sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the logical schedule offset in microseconds.
    #[must_use]
    pub const fn offset_micros(&self) -> u64 {
        self.offset_micros
    }

    /// Returns the subject operation.
    #[must_use]
    pub const fn operation(&self) -> &PlannedOperation {
        &self.operation
    }
}

/// Validated, lazily materialized schedule for one stable workload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QualificationPlan {
    id: StableId,
    kind: PlanKind,
    profile_id: StableId,
    workload: Workload,
}

impl QualificationPlan {
    /// Binds a validated workload to a profile resource envelope.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the workload exceeds the envelope, its logical duration
    /// cannot be represented in microseconds, or a soak is shorter than one hour.
    pub fn new(
        id: StableId,
        kind: PlanKind,
        profile_id: StableId,
        envelope: ResourceEnvelope,
        workload: Workload,
    ) -> Result<Self, QualificationError> {
        workload.validate_against(envelope)?;
        let duration_micros = u128::from(workload.parameters().duration_seconds()) * 1_000_000;
        if duration_micros > u128::from(u64::MAX) {
            return Err(QualificationError::invalid_value(
                "workload.duration_seconds",
                "logical schedule duration does not fit in microseconds",
            ));
        }
        if kind == PlanKind::Soak && workload.parameters().duration_seconds() < 3_600 {
            return Err(QualificationError::invalid_value(
                "soak.duration_seconds",
                "soak plans must cover at least one hour",
            ));
        }
        Ok(Self { id, kind, profile_id, workload })
    }

    /// Returns the stable plan identifier.
    #[must_use]
    pub const fn id(&self) -> &StableId {
        &self.id
    }

    /// Returns the schedule category.
    #[must_use]
    pub const fn kind(&self) -> PlanKind {
        self.kind
    }

    /// Returns the bound profile identifier.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile_id
    }

    /// Returns the stable workload definition.
    #[must_use]
    pub const fn workload(&self) -> &Workload {
        &self.workload
    }

    /// Returns the exact number of lazily generated steps.
    #[must_use]
    pub const fn step_count(&self) -> u64 {
        self.workload.parameters().operation_count()
    }

    /// Returns one deterministic step without materializing the schedule.
    #[must_use]
    pub fn step(&self, sequence: u64) -> Option<PlanStep> {
        if sequence >= self.step_count() {
            return None;
        }
        let parameters = self.workload.parameters();
        let numerator = u128::from(sequence) * 1_000_000;
        let offset = numerator / u128::from(parameters.operations_per_second());
        let offset_micros = u64::try_from(offset).ok()?;
        let operation = operation_for(self.workload.scenario(), parameters, sequence);
        Some(PlanStep { sequence, offset_micros, operation })
    }

    /// Iterates the schedule lazily with constant memory usage.
    #[must_use]
    pub const fn iter(&self) -> PlanIter<'_> {
        PlanIter { plan: self, next: 0 }
    }
}

fn operation_for(
    scenario: ScenarioKind,
    parameters: crate::WorkloadParameters,
    sequence: u64,
) -> PlannedOperation {
    match scenario {
        ScenarioKind::ConcurrentRuns => run_operation(parameters, sequence),
        ScenarioKind::EventAppend => {
            PlannedOperation::AppendEvent { bytes: jittered_payload(parameters, sequence) }
        }
        ScenarioKind::TerminalStreaming => process_operation(parameters, sequence, false),
        ScenarioKind::MemoryBounds => memory_operation(parameters, sequence),
        ScenarioKind::Cancellation => process_operation(parameters, sequence, true),
        ScenarioKind::Recovery => recovery_operation(sequence),
        ScenarioKind::QueueSaturation => queue_operation(QueueKind::Command, parameters, sequence),
        ScenarioKind::ExporterBackpressure => {
            queue_operation(QueueKind::Exporter, parameters, sequence)
        }
        ScenarioKind::ProviderBackpressure => {
            queue_operation(QueueKind::Provider, parameters, sequence)
        }
        ScenarioKind::TokenFlow => provider_operation(parameters, sequence),
        ScenarioKind::DiskArtifacts => disk_operation(parameters, sequence),
    }
}

fn run_operation(parameters: crate::WorkloadParameters, sequence: u64) -> PlannedOperation {
    let concurrency = u64::from(parameters.max_concurrency());
    let cycle = concurrency * 3;
    let phase_offset = sequence % cycle;
    let generation = sequence / cycle;
    let slot = phase_offset % concurrency;
    let run = generation * concurrency + slot;
    match phase_offset / concurrency {
        0 => PlannedOperation::StartRun {
            run,
            memory_bytes: parameters.memory_reservation_bytes(),
            disk_bytes: parameters.disk_reservation_bytes(),
            tokens: parameters.token_reservation(),
        },
        1 => PlannedOperation::AppendEvent { bytes: jittered_payload(parameters, sequence) },
        _ => PlannedOperation::FinishRun { run },
    }
}

fn process_operation(
    parameters: crate::WorkloadParameters,
    sequence: u64,
    cancellation: bool,
) -> PlannedOperation {
    let concurrency = u64::from(parameters.max_concurrency());
    let phases = if cancellation { 4 } else { 3 };
    let cycle = concurrency * phases;
    let phase_offset = sequence % cycle;
    let generation = sequence / cycle;
    let slot = phase_offset % concurrency;
    let process = generation * concurrency + slot;
    match phase_offset / concurrency {
        0 => PlannedOperation::StartProcess {
            process,
            memory_bytes: parameters.memory_reservation_bytes(),
        },
        1 => PlannedOperation::StreamTerminal {
            process,
            bytes: jittered_payload(parameters, sequence),
        },
        2 if cancellation => PlannedOperation::CancelProcess { process },
        _ => PlannedOperation::FinishProcess { process },
    }
}

fn memory_operation(parameters: crate::WorkloadParameters, sequence: u64) -> PlannedOperation {
    let concurrency = u64::from(parameters.max_concurrency());
    let phase = (sequence % (concurrency * 3)) / concurrency;
    if phase == 1 { PlannedOperation::SampleResources } else { run_operation(parameters, sequence) }
}

const fn recovery_operation(sequence: u64) -> PlannedOperation {
    match sequence % 3 {
        0 => PlannedOperation::AppendEvent { bytes: 4096 },
        1 => PlannedOperation::CrashDaemon { journal_events: sequence / 3 + 1 },
        _ => PlannedOperation::RestartDaemon,
    }
}

fn queue_operation(
    queue: QueueKind,
    parameters: crate::WorkloadParameters,
    sequence: u64,
) -> PlannedOperation {
    let capacity = u64::from(parameters.queue_capacity());
    let phase = sequence % (capacity * 2 + 1);
    if sequence.checked_add(1) == Some(parameters.operation_count()) && phase != capacity * 2 {
        let retained = if phase <= capacity { phase } else { capacity * 2 + 1 - phase };
        return PlannedOperation::DrainQueue {
            queue,
            count: u32::try_from(retained).expect("retained depth is at most u32 queue capacity"),
        };
    }
    match phase.cmp(&capacity) {
        Ordering::Less => PlannedOperation::Enqueue { queue, count: 1 },
        Ordering::Equal => PlannedOperation::ObserveBackpressure { queue },
        Ordering::Greater => PlannedOperation::Dequeue { queue, count: 1 },
    }
}

fn provider_operation(parameters: crate::WorkloadParameters, sequence: u64) -> PlannedOperation {
    let concurrency = u64::from(parameters.max_concurrency());
    let cycle = concurrency * 3;
    let phase_offset = sequence % cycle;
    let generation = sequence / cycle;
    let request = generation * concurrency + phase_offset % concurrency;
    match phase_offset / concurrency {
        0 => PlannedOperation::StartProviderRequest { request },
        1 => PlannedOperation::AccountTokens { request, tokens: parameters.token_reservation() },
        _ => PlannedOperation::FinishProviderRequest { request },
    }
}

fn disk_operation(parameters: crate::WorkloadParameters, sequence: u64) -> PlannedOperation {
    match sequence % 3 {
        0 => PlannedOperation::WriteArtifact { bytes: jittered_payload(parameters, sequence) },
        1 => PlannedOperation::SampleResources,
        _ => PlannedOperation::CollectArtifacts,
    }
}

fn jittered_payload(parameters: crate::WorkloadParameters, sequence: u64) -> u32 {
    let base = parameters.payload_bytes();
    let spread = base / 4;
    if spread == 0 {
        return base;
    }
    let mixed = mix64(parameters.seed() ^ sequence);
    let width = u64::from(spread) * 2 + 1;
    let adjustment = i64::try_from(mixed % width).unwrap_or(0) - i64::from(spread);
    u32::try_from(i64::from(base) + adjustment).unwrap_or(base)
}

const fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
