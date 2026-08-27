//! Cooperative cancellation and hard per-case resource ceilings.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{QualificationError, error::invalid};

/// Immutable limits applied to every fresh-subject probe.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QualificationLimits {
    duration_millis: u64,
    processes: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifacts: u32,
}

impl QualificationLimits {
    /// Creates explicit nonzero limits within the H0 hard ceilings.
    ///
    /// # Errors
    ///
    /// Rejects zero values and values above hard allocation and duration ceilings.
    pub fn new(
        max_duration_millis: u64,
        max_processes: u32,
        max_peak_memory_bytes: u64,
        max_output_bytes: u64,
        max_artifacts: u32,
    ) -> Result<Self, QualificationError> {
        if max_duration_millis == 0 || max_duration_millis > 3_600_000 {
            return Err(invalid("per-case duration must be within 1 ms and 1 hour"));
        }
        if max_processes == 0 || max_processes > 1_024 {
            return Err(invalid("per-case process limit must be within 1 and 1024"));
        }
        if max_peak_memory_bytes == 0 || max_peak_memory_bytes > 64 * 1024 * 1024 * 1024 {
            return Err(invalid("per-case peak memory limit is outside the H0 ceiling"));
        }
        if max_output_bytes == 0 || max_output_bytes > 1024 * 1024 * 1024 {
            return Err(invalid("per-case output limit is outside the H0 ceiling"));
        }
        if max_artifacts == 0 || max_artifacts > 4_096 {
            return Err(invalid("per-case artifact limit must be within 1 and 4096"));
        }
        Ok(Self {
            duration_millis: max_duration_millis,
            processes: max_processes,
            peak_memory_bytes: max_peak_memory_bytes,
            output_bytes: max_output_bytes,
            artifacts: max_artifacts,
        })
    }

    /// Returns the production H0 ceilings.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            duration_millis: 300_000,
            processes: 64,
            peak_memory_bytes: 4 * 1024 * 1024 * 1024,
            output_bytes: 64 * 1024 * 1024,
            artifacts: 256,
        }
    }

    /// Returns maximum monotonic elapsed time.
    #[must_use]
    pub const fn max_duration_millis(self) -> u64 {
        self.duration_millis
    }

    /// Returns maximum owned process count.
    #[must_use]
    pub const fn max_processes(self) -> u32 {
        self.processes
    }

    /// Returns maximum observed peak memory.
    #[must_use]
    pub const fn max_peak_memory_bytes(self) -> u64 {
        self.peak_memory_bytes
    }

    /// Returns maximum captured-plus-retained output bytes.
    #[must_use]
    pub const fn max_output_bytes(self) -> u64 {
        self.output_bytes
    }

    /// Returns maximum retained artifact count.
    #[must_use]
    pub const fn max_artifacts(self) -> u32 {
        self.artifacts
    }
}

impl Default for QualificationLimits {
    fn default() -> Self {
        Self::production()
    }
}

/// Direct monotonic resource accounting returned by a native adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceUsage {
    elapsed_millis: u64,
    process_count: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifact_count: u32,
}

impl ResourceUsage {
    /// Creates exact observed resource values.
    #[must_use]
    pub const fn new(
        elapsed_millis: u64,
        process_count: u32,
        peak_memory_bytes: u64,
        output_bytes: u64,
        artifact_count: u32,
    ) -> Self {
        Self { elapsed_millis, process_count, peak_memory_bytes, output_bytes, artifact_count }
    }

    /// Returns monotonic elapsed milliseconds.
    #[must_use]
    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }

    /// Returns maximum simultaneously owned processes.
    #[must_use]
    pub const fn process_count(self) -> u32 {
        self.process_count
    }

    /// Returns measured peak resident memory bytes.
    #[must_use]
    pub const fn peak_memory_bytes(self) -> u64 {
        self.peak_memory_bytes
    }

    /// Returns total bounded output bytes observed before digest retention.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }

    /// Returns retained artifact count.
    #[must_use]
    pub const fn artifact_count(self) -> u32 {
        self.artifact_count
    }

    /// Reports whether every observation is within the supplied hard ceiling.
    #[must_use]
    pub const fn within(self, limits: QualificationLimits) -> bool {
        self.elapsed_millis <= limits.duration_millis
            && self.process_count <= limits.processes
            && self.peak_memory_bytes <= limits.peak_memory_bytes
            && self.output_bytes <= limits.output_bytes
            && self.artifact_count <= limits.artifacts
    }
}

/// Cloneable cooperative cancellation signal owned by one campaign.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates a non-cancelled signal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation for the current and all remaining cases.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Reports whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
