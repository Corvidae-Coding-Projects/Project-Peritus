//! Validated reference-machine and resource-envelope types.

use serde::Serialize;

use crate::{QualificationError, StableId};

/// Reproducible description of the machine class used for qualification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReferenceMachine {
    operating_system: StableId,
    architecture: StableId,
    cpu_model: String,
    logical_cores: u16,
    memory_bytes: u64,
    storage_class: StableId,
}

impl ReferenceMachine {
    /// Constructs a validated reference-machine description.
    ///
    /// # Errors
    /// Returns [`QualificationError`] for invalid CPU text or zero core or memory capacity.
    pub fn new(
        operating_system: StableId,
        architecture: StableId,
        cpu_model: impl Into<String>,
        logical_cores: u16,
        memory_bytes: u64,
        storage_class: StableId,
    ) -> Result<Self, QualificationError> {
        let cpu_model = cpu_model.into();
        if cpu_model.trim().is_empty() || cpu_model.len() > 160 {
            return Err(QualificationError::invalid_value(
                "reference_machine.cpu_model",
                "must contain 1 through 160 bytes",
            ));
        }
        if logical_cores == 0 || memory_bytes == 0 {
            return Err(QualificationError::invalid_value(
                "reference_machine.capacity",
                "cores and memory must be greater than zero",
            ));
        }
        Ok(Self {
            operating_system,
            architecture,
            cpu_model,
            logical_cores,
            memory_bytes,
            storage_class,
        })
    }

    /// Returns the operating-system profile key.
    #[must_use]
    pub const fn operating_system(&self) -> &StableId {
        &self.operating_system
    }

    /// Returns the architecture profile key.
    #[must_use]
    pub const fn architecture(&self) -> &StableId {
        &self.architecture
    }

    /// Returns the recorded CPU model.
    #[must_use]
    pub fn cpu_model(&self) -> &str {
        &self.cpu_model
    }

    /// Returns the logical-core count.
    #[must_use]
    pub const fn logical_cores(&self) -> u16 {
        self.logical_cores
    }

    /// Returns installed memory in bytes.
    #[must_use]
    pub const fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    /// Returns the storage-class profile key.
    #[must_use]
    pub const fn storage_class(&self) -> &StableId {
        &self.storage_class
    }
}

/// Maximum simultaneous subject activities.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ConcurrencyLimits {
    runs: u32,
    processes: u32,
    provider_requests: u32,
}

impl ConcurrencyLimits {
    /// Constructs nonzero concurrency limits.
    ///
    /// # Errors
    /// Returns [`QualificationError`] when any concurrency limit is zero.
    pub const fn new(
        runs: u32,
        processes: u32,
        provider_requests: u32,
    ) -> Result<Self, QualificationError> {
        if runs == 0 || processes == 0 || provider_requests == 0 {
            return Err(QualificationError::invalid_value(
                "resource_envelope.concurrency",
                "all concurrency limits must be greater than zero",
            ));
        }
        Ok(Self { runs, processes, provider_requests })
    }

    /// Returns the active-run bound.
    #[must_use]
    pub const fn runs(self) -> u32 {
        self.runs
    }

    /// Returns the owned-process bound.
    #[must_use]
    pub const fn processes(self) -> u32 {
        self.processes
    }

    /// Returns the provider-request bound.
    #[must_use]
    pub const fn provider_requests(self) -> u32 {
        self.provider_requests
    }
}

/// Maximum retained memory, disk, and token resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CapacityLimits {
    memory_bytes: u64,
    disk_bytes: u64,
    tokens: u64,
}

impl CapacityLimits {
    /// Constructs nonzero capacity limits.
    ///
    /// # Errors
    /// Returns [`QualificationError`] when any memory, disk, or token capacity is zero.
    pub const fn new(
        memory_bytes: u64,
        disk_bytes: u64,
        tokens: u64,
    ) -> Result<Self, QualificationError> {
        if memory_bytes == 0 || disk_bytes == 0 || tokens == 0 {
            return Err(QualificationError::invalid_value(
                "resource_envelope.capacity",
                "all capacity limits must be greater than zero",
            ));
        }
        Ok(Self { memory_bytes, disk_bytes, tokens })
    }

    /// Returns the resident-memory bound.
    #[must_use]
    pub const fn memory_bytes(self) -> u64 {
        self.memory_bytes
    }

    /// Returns the retained-disk bound.
    #[must_use]
    pub const fn disk_bytes(self) -> u64 {
        self.disk_bytes
    }

    /// Returns the token bound.
    #[must_use]
    pub const fn tokens(self) -> u64 {
        self.tokens
    }
}

/// Maximum depths of bounded queues exercised by qualification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct QueueLimits {
    command: u32,
    terminal: u32,
    exporter: u32,
    provider: u32,
}

impl QueueLimits {
    /// Constructs nonzero queue limits.
    ///
    /// # Errors
    /// Returns [`QualificationError`] when any queue capacity is zero.
    pub const fn new(
        command: u32,
        terminal: u32,
        exporter: u32,
        provider: u32,
    ) -> Result<Self, QualificationError> {
        if command == 0 || terminal == 0 || exporter == 0 || provider == 0 {
            return Err(QualificationError::invalid_value(
                "resource_envelope.queues",
                "all queue limits must be greater than zero",
            ));
        }
        Ok(Self { command, terminal, exporter, provider })
    }

    /// Returns the command-queue bound.
    #[must_use]
    pub const fn command(self) -> u32 {
        self.command
    }

    /// Returns the terminal-queue bound.
    #[must_use]
    pub const fn terminal(self) -> u32 {
        self.terminal
    }

    /// Returns the exporter-queue bound.
    #[must_use]
    pub const fn exporter(self) -> u32 {
        self.exporter
    }

    /// Returns the provider-queue bound.
    #[must_use]
    pub const fn provider(self) -> u32 {
        self.provider
    }
}

/// Resource bounds that qualification must exercise without bypass.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ResourceEnvelope {
    concurrency: ConcurrencyLimits,
    capacity: CapacityLimits,
    queues: QueueLimits,
}

impl ResourceEnvelope {
    /// Combines validated concurrency, capacity, and queue limits.
    #[must_use]
    pub const fn new(
        concurrency: ConcurrencyLimits,
        capacity: CapacityLimits,
        queues: QueueLimits,
    ) -> Self {
        Self { concurrency, capacity, queues }
    }

    /// Returns concurrency bounds.
    #[must_use]
    pub const fn concurrency(self) -> ConcurrencyLimits {
        self.concurrency
    }

    /// Returns capacity bounds.
    #[must_use]
    pub const fn capacity(self) -> CapacityLimits {
        self.capacity
    }

    /// Returns bounded queue depths.
    #[must_use]
    pub const fn queues(self) -> QueueLimits {
        self.queues
    }
}
