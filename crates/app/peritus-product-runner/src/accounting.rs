//! Production numeric accounting shared by ordinary execution and Verus.

mod limits;
mod usage;
mod work;

pub use limits::BudgetViolation;
pub use usage::UsageSnapshot;
use vstd::prelude::*;
pub use work::WorkEvent;

verus! {

/// Whether one resource value covers the complete requested observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceMeasurementStatus {
    /// Every requested item was observed and the numeric value is exact for that traversal.
    Measured,
    /// Some requested items were observed; the optional value is only the covered lower bound.
    Partial,
    /// No defensible numeric value is available.
    Unavailable,
}

/// Filesystem or platform operation that prevented a complete resource observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceObservationOperation {
    /// Open a directory for traversal.
    OpenDirectory,
    /// Read the next directory entry.
    ReadDirectoryEntry,
    /// Read one entry's link-safe metadata.
    ReadMetadata,
    /// Read the platform's current-process memory source.
    ReadProcessMemory,
    /// Start the shared resource-observation worker.
    StartResourceObserver,
    /// Start the platform process-memory command.
    StartProcessMemoryCommand,
}

/// Stable I/O classification retained with an optional native error number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceIoErrorKind {
    /// The requested filesystem object does not exist.
    NotFound,
    /// The operating system denied the requested access.
    PermissionDenied,
    /// A connection endpoint rejected the operation.
    ConnectionRefused,
    /// A connection was reset while the operation was pending.
    ConnectionReset,
    /// A connection was aborted while the operation was pending.
    ConnectionAborted,
    /// The operation requires a connection that is not present.
    NotConnected,
    /// The requested address is already occupied.
    AddressInUse,
    /// The requested address is unavailable.
    AddressNotAvailable,
    /// The peer closed a pipe needed by the operation.
    BrokenPipe,
    /// The requested object already exists.
    AlreadyExists,
    /// The operation would block the current observer.
    WouldBlock,
    /// The operating system rejected the input shape.
    InvalidInput,
    /// The operating system returned malformed data.
    InvalidData,
    /// The operating system reported its own timeout.
    TimedOut,
    /// A write reported no forward progress.
    WriteZero,
    /// The operation was interrupted by the operating system.
    Interrupted,
    /// The platform does not support the requested operation.
    Unsupported,
    /// The platform source ended before the complete value arrived.
    UnexpectedEof,
    /// The operating system could not allocate the requested memory.
    OutOfMemory,
    /// The error kind has no stable, more specific representation.
    Other,
}

/// Typed reason why a measurement is partial or unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceObservationCause {
    /// The background observer has not published its first page yet.
    NotObserved,
    /// A legacy durable record contains a numeric value without availability metadata.
    LegacyUnspecified,
    /// The first complete workspace traversal finished after execution admission.
    BaselineEstablishedAfterStart,
    /// The caller deliberately withheld recursive workspace authority.
    NotAuthorized,
    /// A finite traversal page completed and the retained frontier still has work.
    ScanInProgress,
    /// The run or observer owner cancelled the pending observation.
    Cancelled,
    /// A numeric value cannot be represented without truncation.
    ArithmeticOverflow,
    /// The background observer could not be started or its state could not be read.
    WorkerUnavailable,
    /// A platform memory source returned a structurally invalid value.
    InvalidPlatformData,
    /// A platform memory command exited unsuccessfully.
    PlatformCommandFailed {
        /// Native process exit code, or `None` when the process had no numeric exit code.
        exit_code: Option<i32>,
        /// Native Unix termination signal, or `None` when no signal was reported.
        signal: Option<i32>,
    },
    /// One exact class of operating-system I/O failed.
    Io {
        /// Exact observation operation that failed.
        operation: ResourceObservationOperation,
        /// Stable standard-library classification of the I/O failure.
        kind: ResourceIoErrorKind,
        /// Native operating-system error number when one was supplied.
        raw_os_error: Option<i32>,
    },
}

/// Exact traversal coverage associated with one workspace measurement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceObservationCoverage {
    entries_observed: u64,
    directories_completed: u64,
    items_unavailable: u64,
    directories_pending: u64,
}

impl ResourceObservationCoverage {
    /// Records exact traversal counters for one observation boundary.
    #[must_use]
    pub const fn new(
        entries_observed: u64,
        directories_completed: u64,
        items_unavailable: u64,
        directories_pending: u64,
    ) -> Self {
        Self {
            entries_observed,
            directories_completed,
            items_unavailable,
            directories_pending,
        }
    }

    /// Returns the directory entries whose metadata was attempted.
    #[must_use]
    pub const fn entries_observed(self) -> u64 { self.entries_observed }

    /// Returns the directories whose iterators reached their end.
    #[must_use]
    pub const fn directories_completed(self) -> u64 { self.directories_completed }

    /// Returns the entries or directories unavailable to the observer.
    #[must_use]
    pub const fn items_unavailable(self) -> u64 { self.items_unavailable }

    /// Returns the open or retained directory cursors still awaiting traversal.
    #[must_use]
    pub const fn directories_pending(self) -> u64 { self.directories_pending }
}

/// Truthful resource value with its completeness, coverage, and typed cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceMeasurement {
    status: ResourceMeasurementStatus,
    value: Option<u64>,
    coverage: ResourceObservationCoverage,
    cause: Option<ResourceObservationCause>,
}

impl ResourceMeasurement {
    /// Creates a complete measurement for the requested observation.
    #[must_use]
    pub const fn measured(value: u64, coverage: ResourceObservationCoverage) -> Self {
        Self {
            status: ResourceMeasurementStatus::Measured,
            value: Some(value),
            coverage,
            cause: None,
        }
    }

    /// Creates an incomplete measurement with an optional covered lower bound.
    #[must_use]
    pub const fn partial(
        value: Option<u64>,
        coverage: ResourceObservationCoverage,
        cause: ResourceObservationCause,
    ) -> Self {
        Self { status: ResourceMeasurementStatus::Partial, value, coverage, cause: Some(cause) }
    }

    /// Creates an unavailable measurement with no traversal coverage.
    #[must_use]
    pub const fn unavailable(cause: ResourceObservationCause) -> Self {
        Self::unavailable_with_coverage(cause, ResourceObservationCoverage {
            entries_observed: 0,
            directories_completed: 0,
            items_unavailable: 0,
            directories_pending: 0,
        })
    }

    /// Creates an unavailable measurement while retaining exact traversal coverage.
    #[must_use]
    pub const fn unavailable_with_coverage(
        cause: ResourceObservationCause,
        coverage: ResourceObservationCoverage,
    ) -> Self {
        Self {
            status: ResourceMeasurementStatus::Unavailable,
            value: None,
            coverage,
            cause: Some(cause),
        }
    }

    /// Returns whether the value covers the complete requested observation.
    #[must_use]
    pub const fn status(self) -> ResourceMeasurementStatus { self.status }

    /// Exact value for `Measured`, or the covered lower bound for `Partial`.
    #[must_use]
    pub const fn value(self) -> Option<u64> { self.value }

    /// Returns a value only when the requested observation is complete.
    #[must_use]
    pub const fn measured_value(self) -> Option<u64> {
        match self.status {
            ResourceMeasurementStatus::Measured => self.value,
            ResourceMeasurementStatus::Partial | ResourceMeasurementStatus::Unavailable => None,
        }
    }

    /// Returns exact traversal coverage for this observation.
    #[must_use]
    pub const fn coverage(self) -> ResourceObservationCoverage { self.coverage }

    /// Returns the typed reason for an incomplete or unavailable observation.
    #[must_use]
    pub const fn cause(self) -> Option<ResourceObservationCause> { self.cause }
}

impl Default for ResourceMeasurement {
    fn default() -> Self { Self::unavailable(ResourceObservationCause::NotObserved) }
}

/// Aggregate progress for one complete product-run attempt.
///
/// Usage replaces each response snapshot, including a later explicit total that corrects a
/// previously derived total. Resource samples describe completed effect boundaries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProductRunProgress {
    pub(crate) model_requests: u32,
    pub(crate) tool_calls: u32,
    pub(crate) retries: u32,
    pub(crate) provider_failovers: u32,
    pub(crate) compactions: u32,
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) total_tokens: u64,
    pub(crate) provider_cost_microunits: u64,
    pub(crate) usage_observations: u32,
    pub(crate) elapsed_millis: u64,
    pub(crate) workspace_bytes: u64,
    pub(crate) workspace_growth_bytes: u64,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) workspace_measurement: ResourceMeasurement,
    pub(crate) workspace_growth_measurement: ResourceMeasurement,
    pub(crate) peak_rss_measurement: ResourceMeasurement,
}

impl ProductRunProgress {
    /// Specification view of model requests.
    pub closed spec fn spec_model_requests(self) -> u32 { self.model_requests }

    /// Provider attempts admitted, including failed and cancelled requests.
    #[must_use]
    pub const fn model_requests(self) -> (value: u32)
        ensures value == self.spec_model_requests(),
    { self.model_requests }

    /// Specification view of tool calls.
    pub closed spec fn spec_tool_calls(self) -> u32 { self.tool_calls }

    /// Application tool calls completed.
    #[must_use]
    pub const fn tool_calls(self) -> (value: u32)
        ensures value == self.spec_tool_calls(),
    { self.tool_calls }

    /// Specification view of retries.
    pub closed spec fn spec_retries(self) -> u32 { self.retries }

    /// Additional provider attempts caused by checked retry policy.
    #[must_use]
    pub const fn retries(self) -> (value: u32)
        ensures value == self.spec_retries(),
    { self.retries }

    /// Specification view of provider failovers.
    pub closed spec fn spec_provider_failovers(self) -> u32 { self.provider_failovers }

    /// Explicit switches to another configured provider.
    #[must_use]
    pub const fn provider_failovers(self) -> (value: u32)
        ensures value == self.spec_provider_failovers(),
    { self.provider_failovers }

    /// Specification view of compactions.
    pub closed spec fn spec_compactions(self) -> u32 { self.compactions }

    /// Deterministic context compactions applied.
    #[must_use]
    pub const fn compactions(self) -> (value: u32)
        ensures value == self.spec_compactions(),
    { self.compactions }

    /// Specification view of input tokens.
    pub closed spec fn spec_input_tokens(self) -> u64 { self.input_tokens }

    /// Provider-reported input tokens.
    #[must_use]
    pub const fn input_tokens(self) -> (value: u64)
        ensures value == self.spec_input_tokens(),
    { self.input_tokens }

    /// Specification view of cached input tokens.
    pub closed spec fn spec_cached_input_tokens(self) -> u64 { self.cached_input_tokens }

    /// Provider-reported cache-read input tokens.
    #[must_use]
    pub const fn cached_input_tokens(self) -> (value: u64)
        ensures value == self.spec_cached_input_tokens(),
    { self.cached_input_tokens }

    /// Specification view of output tokens.
    pub closed spec fn spec_output_tokens(self) -> u64 { self.output_tokens }

    /// Provider-reported output tokens.
    #[must_use]
    pub const fn output_tokens(self) -> (value: u64)
        ensures value == self.spec_output_tokens(),
    { self.output_tokens }

    /// Specification view of total tokens.
    pub closed spec fn spec_total_tokens(self) -> u64 { self.total_tokens }

    /// Explicit or conservatively derived aggregate tokens.
    #[must_use]
    pub const fn total_tokens(self) -> (value: u64)
        ensures value == self.spec_total_tokens(),
    { self.total_tokens }

    /// Specification view of provider cost microunits.
    pub closed spec fn spec_provider_cost_microunits(self) -> u64 { self.provider_cost_microunits }

    /// Provider-estimated cost in integer microunits.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> (value: u64)
        ensures value == self.spec_provider_cost_microunits(),
    { self.provider_cost_microunits }

    /// Specification view of usage observations.
    pub closed spec fn spec_usage_observations(self) -> u32 { self.usage_observations }

    /// Responses that supplied at least one normalized usage counter.
    #[must_use]
    pub const fn usage_observations(self) -> (value: u32)
        ensures value == self.spec_usage_observations(),
    { self.usage_observations }

    /// Specification view of elapsed millis.
    pub closed spec fn spec_elapsed_millis(self) -> u64 { self.elapsed_millis }

    /// Wall-clock time observed at the latest completed effect boundary.
    #[must_use]
    pub const fn elapsed_millis(self) -> (value: u64)
        ensures value == self.spec_elapsed_millis(),
    { self.elapsed_millis }

    /// Specification view of workspace bytes.
    pub closed spec fn spec_workspace_bytes(self) -> u64 { self.workspace_bytes }

    /// Last complete workspace projection for legacy consumers.
    ///
    /// Zero does not establish that an unavailable or partial observation measured zero; consult
    /// [`Self::workspace_measurement`] before presenting this compatibility field as telemetry.
    #[must_use]
    pub const fn workspace_bytes(self) -> (value: u64)
        ensures value == self.spec_workspace_bytes(),
    { self.workspace_bytes }

    /// Specification view of workspace growth bytes.
    pub closed spec fn spec_workspace_growth_bytes(self) -> u64 { self.workspace_growth_bytes }

    /// Last complete attempt-growth projection for legacy consumers.
    ///
    /// Zero does not establish measured zero; consult [`Self::workspace_growth_measurement`].
    #[must_use]
    pub const fn workspace_growth_bytes(self) -> (value: u64)
        ensures value == self.spec_workspace_growth_bytes(),
    { self.workspace_growth_bytes }

    /// Specification view of peak rss bytes.
    pub closed spec fn spec_peak_rss_bytes(self) -> u64 { self.peak_rss_bytes }

    /// Last defensible high-water projection for legacy consumers.
    ///
    /// Consult [`Self::peak_rss_measurement`] before presenting zero as an observation.
    #[must_use]
    pub const fn peak_rss_bytes(self) -> (value: u64)
        ensures value == self.spec_peak_rss_bytes(),
    { self.peak_rss_bytes }

    /// Completeness and exact traversal coverage for the workspace byte observation.
    #[must_use]
    pub const fn workspace_measurement(self) -> ResourceMeasurement {
        self.workspace_measurement
    }

    /// Completeness and exact traversal coverage for the attempt-growth observation.
    #[must_use]
    pub const fn workspace_growth_measurement(self) -> ResourceMeasurement {
        self.workspace_growth_measurement
    }

    /// Completeness and cause for resident-memory telemetry.
    #[must_use]
    pub const fn peak_rss_measurement(self) -> ResourceMeasurement {
        self.peak_rss_measurement
    }

}

/// Numeric state retained across response snapshots and request boundaries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccountingState {
    pub(crate) progress: ProductRunProgress,
    pub(crate) response_usage: UsageSnapshot,
}

/// Arithmetic rejection before any numeric state is published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountingError {
    /// A token, cost, or work counter cannot represent the requested update.
    CounterOverflow,
    /// The cumulative observation count cannot represent the requested replacement.
    ObservationOverflow,
}

} // verus!
