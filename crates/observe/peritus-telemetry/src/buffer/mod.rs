//! Byte-accounted telemetry buffering and explicit observation-loss policy.

#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-visible buffer internals cross private export and recovery modules"
)]

mod state;

use std::num::NonZeroUsize;

use crate::{TelemetryError, TelemetryErrorKind};

pub use state::TelemetryBuffer;
pub(crate) use state::{
    BufferedRecord, DispositionPrefix, accepted_record_prefix, lost_record_prefix,
};

/// Caller-selected behavior when a canonical observation cannot remain resident in memory.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationLossPolicy {
    /// Reject the arriving observation and retain every pending observation.
    RejectNewest,
    /// Evict the oldest unpinned observations until the arriving observation fits.
    DropOldest,
    /// Persist overflow observations to a caller-owned durable spill store without loss.
    LosslessSpill,
}

/// Reason an arriving observation was explicitly lost.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RejectionReason {
    /// No configured resident page was available.
    MemoryFull,
    /// The observation needs more resident pages than the complete memory budget.
    RecordExceedsMemory,
    /// Making room would have required evicting an in-flight export prefix.
    InFlightPrefix,
}

/// Validated physical-memory and export-batch limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferConfig {
    memory_page_bytes: NonZeroUsize,
    memory_pages: NonZeroUsize,
    memory_capacity_bytes: NonZeroUsize,
    batch_bytes: NonZeroUsize,
    loss_policy: ObservationLossPolicy,
}

impl BufferConfig {
    /// Creates positive physical-page and per-batch byte limits.
    ///
    /// No cumulative record or work ceiling is imposed. Every resident record is charged in whole
    /// caller-sized pages, and each batch is byte-bounded while still admitting one oversized
    /// record so a durable spill can always make progress.
    ///
    /// # Errors
    ///
    /// Rejects a page-size and page-count product that cannot be represented by this process.
    pub fn new(
        memory_page_bytes: NonZeroUsize,
        memory_pages: NonZeroUsize,
        batch_bytes: NonZeroUsize,
        loss_policy: ObservationLossPolicy,
    ) -> Result<Self, TelemetryError> {
        let memory_capacity_bytes = memory_page_bytes
            .get()
            .checked_mul(memory_pages.get())
            .and_then(NonZeroUsize::new)
            .ok_or_else(|| {
                TelemetryError::new(
                    TelemetryErrorKind::InvalidConfiguration,
                    "validate telemetry buffer",
                    "physical memory page product is not representable",
                )
            })?;
        Ok(Self {
            memory_page_bytes,
            memory_pages,
            memory_capacity_bytes,
            batch_bytes,
            loss_policy,
        })
    }

    /// Returns the caller's physical page size in bytes.
    #[must_use]
    pub const fn memory_page_bytes(self) -> NonZeroUsize {
        self.memory_page_bytes
    }
    /// Returns the positive number of resident pages.
    #[must_use]
    pub const fn memory_pages(self) -> NonZeroUsize {
        self.memory_pages
    }
    /// Returns the checked resident-memory budget in bytes.
    #[must_use]
    pub const fn memory_capacity_bytes(self) -> NonZeroUsize {
        self.memory_capacity_bytes
    }
    /// Returns the target canonical payload bytes per export batch.
    #[must_use]
    pub const fn batch_bytes(self) -> NonZeroUsize {
        self.batch_bytes
    }
    /// Returns the explicit observation-loss policy.
    #[must_use]
    pub const fn loss_policy(self) -> ObservationLossPolicy {
        self.loss_policy
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
    /// Returns records historically accepted into resident memory or durable spill.
    #[must_use]
    pub const fn accepted(self) -> u64 {
        self.accepted
    }
    /// Returns records explicitly rejected or evicted under a lossy policy.
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
                "checkpoint counters violate observation accounting",
            ));
        }
        Ok(Self { submitted, accepted, dropped, exported })
    }
}

/// Result of one enqueue operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EnqueueOutcome {
    /// Observation was accepted into byte-accounted resident memory.
    Accepted {
        /// Stable submitted sequence.
        sequence: u64,
        /// Physical page bytes charged to this resident observation.
        resident_bytes: usize,
    },
    /// Observation was accepted only after older observations were explicitly lost.
    DroppedOldest {
        /// Accepted arriving sequence.
        accepted_sequence: u64,
        /// First evicted stable sequence.
        first_dropped_sequence: u64,
        /// Last evicted stable sequence.
        last_dropped_sequence: u64,
        /// Exact number of evicted observations.
        count: u64,
    },
    /// Observation was durably accepted outside resident memory.
    Spilled {
        /// Stable submitted sequence.
        sequence: u64,
        /// Exact canonical payload bytes held by the spill record.
        canonical_bytes: u64,
    },
    /// Arriving observation was explicitly lost.
    RejectedNewest {
        /// Rejected stable submitted sequence.
        rejected_sequence: u64,
        /// Why the configured policy could not retain it.
        reason: RejectionReason,
    },
}
