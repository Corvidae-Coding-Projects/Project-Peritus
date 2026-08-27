//! Public resource-accounting observation and report types.

use serde::{Deserialize, Serialize};

/// Bounded queue whose depth and producer wait are measured.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueKind {
    /// Serialized authoritative command intake.
    Command,
    /// Terminal output delivery.
    Terminal,
    /// Telemetry exporter spool.
    Exporter,
    /// Provider request dispatch.
    Provider,
}

/// One exact resource-lifecycle observation from a runner adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceEvent {
    /// A run acquired its declared reservations.
    RunStarted {
        /// Runner-local run number.
        run: u64,
        /// Resident bytes reserved for the run.
        memory_bytes: u64,
        /// Disk bytes reserved for the run.
        disk_bytes: u64,
        /// Tokens reserved for the run.
        tokens: u64,
    },
    /// A run terminated and released its reservations.
    RunFinished {
        /// Runner-local run number.
        run: u64,
    },
    /// An owned process acquired a slot and resident-memory reservation.
    ProcessStarted {
        /// Runner-local process number.
        process: u64,
        /// Resident bytes reserved for the process.
        memory_bytes: u64,
    },
    /// An owned process terminated and released its reservation.
    ProcessFinished {
        /// Runner-local process number.
        process: u64,
    },
    /// A provider request acquired a provider slot.
    ProviderRequestStarted {
        /// Runner-local request number.
        request: u64,
    },
    /// A provider request released its provider slot.
    ProviderRequestFinished {
        /// Runner-local request number.
        request: u64,
    },
    /// Items entered a bounded queue.
    QueuePushed {
        /// Queue being changed.
        queue: QueueKind,
        /// Number of added items.
        count: u32,
    },
    /// Items left a bounded queue.
    QueuePopped {
        /// Queue being changed.
        queue: QueueKind,
        /// Number of removed items.
        count: u32,
    },
    /// A producer observed a full queue and measured its wait.
    BackpressureObserved {
        /// Full queue.
        queue: QueueKind,
        /// Producer wait in microseconds.
        wait_micros: u64,
    },
    /// Durable artifact bytes were retained outside a run reservation.
    DiskRetained {
        /// Newly retained bytes.
        bytes: u64,
    },
    /// Durable artifact bytes were released by collection.
    DiskReleased {
        /// Released bytes.
        bytes: u64,
    },
    /// Tokens were consumed outside a run reservation.
    TokensConsumed {
        /// Newly consumed tokens.
        tokens: u64,
    },
}

/// High-water and terminal resource evidence from one qualification run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccountingSummary {
    pub(crate) peak_runs: u32,
    pub(crate) peak_processes: u32,
    pub(crate) peak_provider_requests: u32,
    pub(crate) peak_memory_bytes: u64,
    pub(crate) peak_disk_bytes: u64,
    pub(crate) peak_tokens: u64,
    pub(crate) peak_command_queue: u32,
    pub(crate) peak_terminal_queue: u32,
    pub(crate) peak_exporter_queue: u32,
    pub(crate) peak_provider_queue: u32,
    pub(crate) command_backpressure_micros: u64,
    pub(crate) terminal_backpressure_micros: u64,
    pub(crate) exporter_backpressure_micros: u64,
    pub(crate) provider_backpressure_micros: u64,
    pub(crate) saturation_observations: u64,
    pub(crate) outstanding_runs: usize,
    pub(crate) outstanding_processes: usize,
    pub(crate) outstanding_provider_requests: usize,
    pub(crate) nonempty_queues: usize,
    pub(crate) retained_memory_bytes: u64,
    pub(crate) retained_disk_bytes: u64,
    pub(crate) retained_tokens: u64,
}

impl AccountingSummary {
    /// Returns whether every acquired lifecycle resource and queued item was released.
    #[must_use]
    pub const fn is_balanced(&self) -> bool {
        self.outstanding_runs == 0
            && self.outstanding_processes == 0
            && self.outstanding_provider_requests == 0
            && self.nonempty_queues == 0
            && self.retained_memory_bytes == 0
    }

    /// Returns the maximum active-run count.
    #[must_use]
    pub const fn peak_runs(&self) -> u32 {
        self.peak_runs
    }

    /// Returns the maximum owned-process count.
    #[must_use]
    pub const fn peak_processes(&self) -> u32 {
        self.peak_processes
    }

    /// Returns the maximum active provider-request count.
    #[must_use]
    pub const fn peak_provider_requests(&self) -> u32 {
        self.peak_provider_requests
    }

    /// Returns peak accounted memory bytes.
    #[must_use]
    pub const fn peak_memory_bytes(&self) -> u64 {
        self.peak_memory_bytes
    }

    /// Returns peak accounted disk bytes.
    #[must_use]
    pub const fn peak_disk_bytes(&self) -> u64 {
        self.peak_disk_bytes
    }

    /// Returns peak accounted tokens.
    #[must_use]
    pub const fn peak_tokens(&self) -> u64 {
        self.peak_tokens
    }

    /// Returns the maximum depth observed for a queue.
    #[must_use]
    pub const fn peak_queue(&self, queue: QueueKind) -> u32 {
        match queue {
            QueueKind::Command => self.peak_command_queue,
            QueueKind::Terminal => self.peak_terminal_queue,
            QueueKind::Exporter => self.peak_exporter_queue,
            QueueKind::Provider => self.peak_provider_queue,
        }
    }

    /// Returns accumulated producer wait for a queue.
    #[must_use]
    pub const fn backpressure_micros(&self, queue: QueueKind) -> u64 {
        match queue {
            QueueKind::Command => self.command_backpressure_micros,
            QueueKind::Terminal => self.terminal_backpressure_micros,
            QueueKind::Exporter => self.exporter_backpressure_micros,
            QueueKind::Provider => self.provider_backpressure_micros,
        }
    }

    /// Returns the number of observations taken while a queue was exactly full.
    #[must_use]
    pub const fn saturation_observations(&self) -> u64 {
        self.saturation_observations
    }
}
