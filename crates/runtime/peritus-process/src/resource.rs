//! Process resource ceilings and observations.

use crate::{ProcessError, error::invalid};

/// Complete supervisor and backend resource ceiling set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessResourcePolicy {
    wall_millis: Option<u64>,
    cpu_millis: Option<u64>,
    memory_bytes: Option<u64>,
    disk_bytes: Option<u64>,
    output_bytes: Option<u64>,
    process_count: Option<u64>,
    file_descriptors: Option<u64>,
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
        Self::with_optional_time(
            Some(wall_millis),
            Some(cpu_millis),
            memory_bytes,
            disk_bytes,
            output_bytes,
            process_count,
            file_descriptors,
            concurrent_slots,
        )
    }

    /// Creates nonzero resource ceilings with optional wall and CPU time bounds.
    ///
    /// # Errors
    /// Rejects any zero configured ceiling.
    #[allow(clippy::too_many_arguments)]
    pub const fn with_optional_time(
        wall_millis: Option<u64>,
        cpu_millis: Option<u64>,
        memory_bytes: u64,
        disk_bytes: u64,
        output_bytes: u64,
        process_count: u64,
        file_descriptors: u64,
        concurrent_slots: u64,
    ) -> Result<Self, ProcessError> {
        Self::with_optional_limits(
            wall_millis,
            cpu_millis,
            Some(memory_bytes),
            Some(disk_bytes),
            Some(output_bytes),
            Some(process_count),
            Some(file_descriptors),
            concurrent_slots,
        )
    }

    /// Creates a process policy with explicit optional cumulative allowances.
    ///
    /// Concurrency is a selected physical allocation. Every other `None` means that the
    /// supervisor observes the dimension without enforcing a synthetic total-work ceiling.
    ///
    /// # Errors
    /// Rejects a selected zero ceiling or zero concurrency.
    #[allow(clippy::too_many_arguments)]
    pub const fn with_optional_limits(
        wall_millis: Option<u64>,
        cpu_millis: Option<u64>,
        memory_bytes: Option<u64>,
        disk_bytes: Option<u64>,
        output_bytes: Option<u64>,
        process_count: Option<u64>,
        file_descriptors: Option<u64>,
        concurrent_slots: u64,
    ) -> Result<Self, ProcessError> {
        if matches!(wall_millis, Some(0))
            || matches!(cpu_millis, Some(0))
            || matches!(memory_bytes, Some(0))
            || matches!(disk_bytes, Some(0))
            || matches!(output_bytes, Some(0))
            || matches!(process_count, Some(0))
            || matches!(file_descriptors, Some(0))
            || concurrent_slots == 0
        {
            return Err(invalid("selected process resource ceilings must be nonzero"));
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
    pub const fn wall_millis(self) -> Option<u64> {
        self.wall_millis
    }
    /// Returns the CPU-time ceiling.
    #[must_use]
    pub const fn cpu_millis(self) -> Option<u64> {
        self.cpu_millis
    }
    /// Returns the memory ceiling.
    #[must_use]
    pub const fn memory_bytes(self) -> u64 {
        match self.memory_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional memory ceiling.
    #[must_use]
    pub const fn memory_limit(self) -> Option<u64> {
        self.memory_bytes
    }
    /// Returns the disk ceiling.
    #[must_use]
    pub const fn disk_bytes(self) -> u64 {
        match self.disk_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional disk ceiling.
    #[must_use]
    pub const fn disk_limit(self) -> Option<u64> {
        self.disk_bytes
    }
    /// Returns the output ceiling.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        match self.output_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative output ceiling.
    #[must_use]
    pub const fn output_limit(self) -> Option<u64> {
        self.output_bytes
    }
    /// Returns the process-count ceiling.
    #[must_use]
    pub const fn process_count(self) -> u64 {
        match self.process_count {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional process-count ceiling.
    #[must_use]
    pub const fn process_limit(self) -> Option<u64> {
        self.process_count
    }
    /// Returns the file-descriptor or handle ceiling.
    #[must_use]
    pub const fn file_descriptors(self) -> u64 {
        match self.file_descriptors {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional file-descriptor or handle ceiling.
    #[must_use]
    pub const fn file_descriptor_limit(self) -> Option<u64> {
        self.file_descriptors
    }
    /// Returns the concurrent-slot ceiling.
    #[must_use]
    pub const fn concurrent_slots(self) -> u64 {
        self.concurrent_slots
    }

    pub(crate) const fn has_selected_native_limits(self) -> bool {
        self.memory_bytes.is_some()
            && self.disk_bytes.is_some()
            && self.output_bytes.is_some()
            && self.process_count.is_some()
            && self.file_descriptors.is_some()
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
    ceiling: Option<u64>,
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
        Self::with_optional_ceiling(dimension, value, Some(ceiling), fidelity)
    }

    /// Creates an observation whose time ceiling may be absent.
    #[must_use]
    pub const fn with_optional_ceiling(
        dimension: ProcessResourceDimension,
        value: u64,
        ceiling: Option<u64>,
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
    pub const fn ceiling(self) -> Option<u64> {
        self.ceiling
    }
    /// Returns observation fidelity.
    #[must_use]
    pub const fn fidelity(self) -> ResourceFidelity {
        self.fidelity
    }
}
