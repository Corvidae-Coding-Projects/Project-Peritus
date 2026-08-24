//! Process resource ceilings and observations.

use crate::{ProcessError, error::invalid};

/// Complete supervisor and backend resource ceiling set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessResourcePolicy {
    wall_millis: u64,
    cpu_millis: u64,
    memory_bytes: u64,
    disk_bytes: u64,
    output_bytes: u64,
    process_count: u64,
    file_descriptors: u64,
    concurrent_slots: u64,
}

impl ProcessResourcePolicy {
    /// Creates finite, nonzero process resource ceilings.
    ///
    /// # Errors
    ///
    /// Returns an error when any hard ceiling is zero.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        wall_millis: u64,
        cpu_millis: u64,
        memory_bytes: u64,
        disk_bytes: u64,
        output_bytes: u64,
        process_count: u64,
        file_descriptors: u64,
        concurrent_slots: u64,
    ) -> Result<Self, ProcessError> {
        if wall_millis == 0
            || cpu_millis == 0
            || memory_bytes == 0
            || disk_bytes == 0
            || output_bytes == 0
            || process_count == 0
            || file_descriptors == 0
            || concurrent_slots == 0
        {
            return Err(invalid("process resource ceilings must all be nonzero"));
        }
        Ok(Self {
            wall_millis,
            cpu_millis,
            memory_bytes,
            disk_bytes,
            output_bytes,
            process_count,
            file_descriptors,
            concurrent_slots,
        })
    }

    /// Returns the wall-time ceiling.
    #[must_use]
    pub const fn wall_millis(self) -> u64 {
        self.wall_millis
    }
    /// Returns the CPU-time ceiling.
    #[must_use]
    pub const fn cpu_millis(self) -> u64 {
        self.cpu_millis
    }
    /// Returns the memory ceiling.
    #[must_use]
    pub const fn memory_bytes(self) -> u64 {
        self.memory_bytes
    }
    /// Returns the disk ceiling.
    #[must_use]
    pub const fn disk_bytes(self) -> u64 {
        self.disk_bytes
    }
    /// Returns the output ceiling.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }
    /// Returns the process-count ceiling.
    #[must_use]
    pub const fn process_count(self) -> u64 {
        self.process_count
    }
    /// Returns the file-descriptor or handle ceiling.
    #[must_use]
    pub const fn file_descriptors(self) -> u64 {
        self.file_descriptors
    }
    /// Returns the concurrent-slot ceiling.
    #[must_use]
    pub const fn concurrent_slots(self) -> u64 {
        self.concurrent_slots
    }
}

/// Fidelity of one terminal resource observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceFidelity {
    /// The backend enforced the ceiling.
    Enforced,
    /// The supervisor sampled the dimension.
    Sampled,
    /// The platform cannot report this dimension.
    Unsupported,
    /// Observation started but did not complete.
    Incomplete,
}

/// Process-local resource dimensions captured in a terminal result.
///
/// This vocabulary is intentionally distinct from the harness-wide budget
/// dimensions in `peritus-types`: every variant describes an operating-system
/// process resource that the C2 supervisor can enforce or observe.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProcessResourceDimension {
    /// Elapsed wall-clock time in milliseconds.
    WallTimeMilliseconds,
    /// Consumed CPU time in milliseconds.
    CpuTimeMilliseconds,
    /// Resident or committed memory in bytes.
    MemoryBytes,
    /// Filesystem storage consumed in bytes.
    DiskBytes,
    /// Combined observed process output in bytes.
    OutputBytes,
    /// Processes in the owned process tree.
    ProcessCount,
    /// Open file descriptors or operating-system handles.
    OpenHandles,
    /// Concurrent execution slots held by the process.
    ConcurrencySlots,
}

/// One typed terminal resource observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessResourceObservation {
    dimension: ProcessResourceDimension,
    value: u64,
    ceiling: u64,
    fidelity: ResourceFidelity,
}

impl ProcessResourceObservation {
    /// Creates an exact resource observation.
    #[must_use]
    pub const fn new(
        dimension: ProcessResourceDimension,
        value: u64,
        ceiling: u64,
        fidelity: ResourceFidelity,
    ) -> Self {
        Self { dimension, value, ceiling, fidelity }
    }

    /// Returns the resource dimension.
    #[must_use]
    pub const fn dimension(self) -> ProcessResourceDimension {
        self.dimension
    }
    /// Returns the observed quantity.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }
    /// Returns the configured ceiling.
    #[must_use]
    pub const fn ceiling(self) -> u64 {
        self.ceiling
    }
    /// Returns observation fidelity.
    #[must_use]
    pub const fn fidelity(self) -> ResourceFidelity {
        self.fidelity
    }
}
