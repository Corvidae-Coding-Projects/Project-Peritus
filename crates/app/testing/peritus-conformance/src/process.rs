//! Runtime-neutral C2 process-owner conformance contract.

mod cases;
mod fixtures;
mod observation;
mod stream;

pub use cases::process_suite;
pub use observation::{
    ProcessConformanceObservation, ProcessEffectObservation, ProcessInvocationObservation,
    ProcessOutputObservation, ProcessOwnershipObservation,
};
pub use stream::{ProcessOutputStream, ProcessStreamOffsetObservation};

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessConformanceError {
    /// The process boundary could not be exercised or observed.
    Infrastructure,
}

/// Process I/O topology requested by a fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessIoMode {
    /// Separate standard-output and standard-error pipes.
    Pipes,
    /// One combined pseudo-terminal stream.
    Pty,
}

/// Durable process-recovery probe supplied to the subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessRecoveryProbe {
    /// The durable record already contains an exact terminal observation.
    Terminal,
    /// The durable identity resolves to the exact live owned tree.
    ExactLive,
    /// The owned identity is absent without a terminal observation.
    Absent,
    /// The platform identity resolves to a different process.
    Mismatched,
    /// The platform cannot establish identity safely.
    Unverifiable,
}

/// Stable recovery classification returned by a subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessRecoveryDisposition {
    /// An exact durable terminal record was recovered.
    Terminal,
    /// The exact live tree remains owned by the supervisor.
    LiveOwned,
    /// The process is absent but no terminal result can be inferred.
    AbsentUnobserved,
    /// Recovery cannot safely determine or control the process.
    Indeterminate,
}

/// Independently varied authority dimension used by the no-effect case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessAuthorizationDrift {
    /// Action identity or digest differs.
    Action,
    /// Canonical execution-intent payload differs.
    IntentPayload,
    /// Intent media type differs.
    MediaType,
    /// Project/run/attempt/turn ownership lineage differs.
    OwnerLineage,
    /// Actor or compiled role differs.
    ActorRole,
    /// Environment identity differs.
    Environment,
    /// Target resource differs.
    Resource,
    /// Capability permission or transition differs.
    Capability,
    /// Budget reservation or active-effect ceiling differs.
    Budget,
    /// Compiled operation class differs.
    OperationClass,
    /// Committed dispatch frame differs or is absent.
    Dispatch,
    /// Any exact revision component differs.
    Revision,
    /// Expected workspace generation differs.
    Generation,
    /// Lease claim, holder, session, or scope differs.
    HolderLease,
    /// Authority epoch, tick, or validity differs.
    AuthorityTime,
    /// Abstract sandbox digest differs.
    SandboxDigest,
    /// Backend descriptor, support, or preparation digest differs.
    BackendPreparation,
}

/// One independently exercised process behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessScenario {
    /// Structured argv, working directory, and environment are observed literally.
    LiteralInvocation,
    /// Separate pipe streams and bounded stdin are exercised.
    PipeStreaming,
    /// Combined PTY input, output, close, and resize are exercised.
    PtyStreaming,
    /// Output reaches a configured exact byte ceiling.
    OutputBound,
    /// User cancellation drives graceful owned-tree shutdown.
    Cancellation,
    /// A deadline drives graceful then forced escalation.
    Deadline,
    /// A root with descendants is terminated and completely joined.
    TreeCleanup,
    /// Concurrent terminal observations are reduced to one result.
    TerminalUniqueness,
    /// One durable restart classification is exercised.
    Restart(ProcessRecoveryProbe),
    /// One authority mismatch must produce no target effect.
    Authorization(ProcessAuthorizationDrift),
}

/// First accepted terminal trigger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessTrigger {
    /// Explicit user cancellation.
    User,
    /// Configured deadline elapsed.
    Deadline,
    /// Configured output ceiling was reached.
    OutputLimit,
}

/// Stable top-level process disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessDisposition {
    /// The child exited normally.
    Exited,
    /// Explicit cancellation won terminal classification.
    Cancelled,
    /// Deadline expiry won terminal classification.
    TimedOut,
    /// Output policy terminated the child.
    OutputLimit,
    /// Authorization was rejected before effect.
    Unauthorized,
    /// A durable record was classified during restart.
    Recovered,
}

/// Literal environment entry requested by a process fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessEnvironmentBinding {
    name: &'static str,
    value: &'static str,
}

impl ProcessEnvironmentBinding {
    /// Creates one fixed conformance binding.
    #[must_use]
    pub const fn new(name: &'static str, value: &'static str) -> Self {
        Self { name, value }
    }

    /// Returns the portable environment name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the literal environment value.
    #[must_use]
    pub const fn value(&self) -> &'static str {
        self.value
    }
}

/// Complete fixed request supplied by one C2 process conformance case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessConformanceFixture {
    scenario: ProcessScenario,
    executable: &'static str,
    arguments: &'static [&'static str],
    working_directory: &'static str,
    environment: &'static [ProcessEnvironmentBinding],
    stdin: &'static [u8],
    io_mode: ProcessIoMode,
    output_limit: u64,
    descendant_depth: u64,
}

impl ProcessConformanceFixture {
    #[allow(
        clippy::too_many_arguments,
        reason = "the fixture keeps independent launch facts explicit"
    )]
    pub(crate) const fn new(
        scenario: ProcessScenario,
        executable: &'static str,
        arguments: &'static [&'static str],
        working_directory: &'static str,
        environment: &'static [ProcessEnvironmentBinding],
        stdin: &'static [u8],
        io_mode: ProcessIoMode,
        output_limit: u64,
        descendant_depth: u64,
    ) -> Self {
        Self {
            scenario,
            executable,
            arguments,
            working_directory,
            environment,
            stdin,
            io_mode,
            output_limit,
            descendant_depth,
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(&self) -> ProcessScenario {
        self.scenario
    }
    /// Returns the executable identity, never a shell command line.
    #[must_use]
    pub const fn executable(&self) -> &'static str {
        self.executable
    }
    /// Returns the ordered literal arguments.
    #[must_use]
    pub const fn arguments(&self) -> &'static [&'static str] {
        self.arguments
    }
    /// Returns the checked working-directory fixture.
    #[must_use]
    pub const fn working_directory(&self) -> &'static str {
        self.working_directory
    }
    /// Returns the complete explicit environment.
    #[must_use]
    pub const fn environment(&self) -> &'static [ProcessEnvironmentBinding] {
        self.environment
    }
    /// Returns the bounded input bytes.
    #[must_use]
    pub const fn stdin(&self) -> &'static [u8] {
        self.stdin
    }
    /// Returns the requested I/O topology.
    #[must_use]
    pub const fn io_mode(&self) -> ProcessIoMode {
        self.io_mode
    }
    /// Returns the exact retained-output ceiling.
    #[must_use]
    pub const fn output_limit(&self) -> u64 {
        self.output_limit
    }
    /// Returns the requested descendant depth.
    #[must_use]
    pub const fn descendant_depth(&self) -> u64 {
        self.descendant_depth
    }
}

/// Adapter implemented by a production C2 process owner under conformance test.
pub trait ProcessConformanceSubject: Send {
    /// Exercises one fixed request and returns direct observations rather than a claimed verdict.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &ProcessConformanceFixture,
    ) -> Result<ProcessConformanceObservation, ProcessConformanceError>;
}
