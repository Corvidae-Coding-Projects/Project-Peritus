//! Deterministic bounded buffer and drop accounting.

#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-visible buffer internals cross private export and recovery modules"
)]

mod state;

use std::num::NonZeroUsize;

use crate::{TelemetryError, TelemetryErrorKind};

pub use state::TelemetryBuffer;
pub(crate) use state::{BufferedRecord, record_prefix};

/// Hard upper bound preventing accidental giant in-memory queues.
pub const MAX_BUFFER_ITEMS: usize = 1_000_000;

/// Deterministic behavior when the queue is full.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackpressurePolicy {
    /// Reject the arriving record and retain every queued record.
    RejectNewest,
    /// Evict exactly one oldest record and accept the arriving record.
    DropOldest,
}

/// Validated queue and batch limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferConfig {
    capacity: NonZeroUsize,
    batch_size: NonZeroUsize,
    policy: BackpressurePolicy,
}

impl BufferConfig {
    /// Creates fixed nonzero queue and batch limits.
    ///
    /// # Errors
    ///
    /// Rejects a capacity above one million or a batch larger than the queue.
    pub const fn new(
        capacity: NonZeroUsize,
        batch_size: NonZeroUsize,
        policy: BackpressurePolicy,
    ) -> Result<Self, TelemetryError> {
        if capacity.get() > MAX_BUFFER_ITEMS || batch_size.get() > capacity.get() {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "validate telemetry buffer",
                "capacity exceeds the hard bound or batch exceeds capacity",
            ));
        }
        Ok(Self { capacity, batch_size, policy })
    }

    /// Returns queue capacity.
    #[must_use]
    pub const fn capacity(self) -> usize {
        self.capacity.get()
    }
    /// Returns maximum records per export batch.
    #[must_use]
    pub const fn batch_size(self) -> usize {
        self.batch_size.get()
    }
    /// Returns deterministic full-queue policy.
    #[must_use]
    pub const fn policy(self) -> BackpressurePolicy {
        self.policy
    }
}

/// Monotonic queue counters.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct BufferCounters {
    submitted: u64,
    accepted: u64,
    dropped: u64,
    exported: u64,
}

impl BufferCounters {
    /// Returns submitted record count and latest stable sequence.
    #[must_use]
    pub const fn submitted(self) -> u64 {
        self.submitted
    }
    /// Returns records historically accepted into the queue.
    #[must_use]
    pub const fn accepted(self) -> u64 {
        self.accepted
    }
    /// Returns records rejected or evicted.
    #[must_use]
    pub const fn dropped(self) -> u64 {
        self.dropped
    }
    /// Returns records explicitly acknowledged by an exporter.
    #[must_use]
    pub const fn exported(self) -> u64 {
        self.exported
    }

    pub(crate) const fn from_parts(
        submitted: u64,
        accepted: u64,
        dropped: u64,
        exported: u64,
    ) -> Result<Self, TelemetryError> {
        if accepted > submitted
            || dropped > submitted
            || submitted.saturating_sub(accepted) > dropped
            || exported > accepted
        {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidCheckpoint,
                "restore telemetry counters",
                "checkpoint counters violate bounded accounting",
            ));
        }
        Ok(Self { submitted, accepted, dropped, exported })
    }
}

/// Result of one enqueue operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EnqueueOutcome {
    /// Record was accepted without eviction.
    Accepted {
        /// Stable submitted sequence.
        sequence: u64,
    },
    /// Oldest record was evicted and the arriving record accepted.
    DroppedOldest {
        /// Accepted arriving sequence.
        accepted_sequence: u64,
        /// Evicted record sequence.
        dropped_sequence: u64,
    },
    /// Arriving record was rejected.
    RejectedNewest {
        /// Rejected stable submitted sequence.
        rejected_sequence: u64,
    },
}
