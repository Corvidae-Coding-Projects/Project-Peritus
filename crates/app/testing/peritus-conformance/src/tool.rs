//! Runtime-neutral C4 tool protocol and router conformance contract.

mod cases;
mod fixtures;
mod observation;

pub use cases::tool_suite;
pub use observation::{
    ToolConformanceObservation, ToolDescriptorObservation, ToolEffectObservation,
    ToolReplayObservation, ToolResultObservation,
};

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolConformanceError {
    /// The tool boundary could not be exercised or observed.
    Infrastructure,
}

/// One independently varied authority dimension used by no-effect checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolAuthorizationDrift {
    /// Action identity or canonical action digest differs.
    Action,
    /// Registered descriptor or schema digest differs.
    Descriptor,
    /// Canonical arguments or prepared-call digest differs.
    Arguments,
    /// B1 operation classification differs.
    OperationClass,
    /// Actor identity or compiled role differs.
    ActorRole,
    /// Target resource or environment differs.
    Resource,
    /// Capability permission or committed use differs.
    Capability,
    /// Budget reservation differs or is missing.
    Budget,
    /// Required lease binding differs or is missing.
    Lease,
    /// B0 dispatch event differs or is absent.
    Dispatch,
    /// Any revision component differs.
    Revision,
    /// Authority epoch, validity, or deadline differs.
    AuthorityTime,
}

/// Replay behavior under test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolReplayMode {
    /// An exact idempotent terminal call is repeated.
    ExactIdempotent,
    /// The same action identity is reused with different bound bytes.
    Conflicting,
    /// A non-idempotent terminal call is repeated.
    NonIdempotent,
    /// Prior effect completion cannot be established safely.
    Indeterminate,
}

/// One independently exercised tool behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolScenario {
    /// Descriptor and schema bytes are generated deterministically.
    DescriptorSchema,
    /// Invalid and over-limit arguments are rejected before authorization.
    SchemaRejection,
    /// Role and capability intersection produces canonical exposure.
    Exposure,
    /// A fully authorized call reaches its exact dispatcher once.
    Dispatch,
    /// One authority mismatch must produce no target effect.
    Authorization(ToolAuthorizationDrift),
    /// Failure remains structured and cannot be hidden by prose.
    ResultTruth,
    /// Ordered progress and explicit cancellation are mediated by the router.
    Cancellation,
    /// Deadline expiry produces one owned non-success terminal result.
    Deadline,
    /// One replay classification is exercised.
    Replay(ToolReplayMode),
}

/// Stable terminal classification returned by a subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolDisposition {
    /// The exact dispatcher completed successfully.
    Succeeded,
    /// Schema, exposure, or authority rejected the call before effect.
    Rejected,
    /// The dispatcher or lower subsystem failed explicitly.
    Failed,
    /// Explicit cancellation won terminal classification.
    Cancelled,
    /// The call deadline won terminal classification.
    TimedOut,
    /// A prior exact result was returned without a second effect.
    Replayed,
    /// Prior effect state could not be established safely.
    Indeterminate,
}

/// Complete fixed request supplied by one C4 tool conformance case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolConformanceFixture {
    scenario: ToolScenario,
    tool_name: &'static str,
    canonical_arguments: &'static [u8],
    output_limit: u64,
    deadline_millis: u64,
}

impl ToolConformanceFixture {
    pub(crate) const fn new(
        scenario: ToolScenario,
        tool_name: &'static str,
        canonical_arguments: &'static [u8],
        output_limit: u64,
        deadline_millis: u64,
    ) -> Self {
        Self { scenario, tool_name, canonical_arguments, output_limit, deadline_millis }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(&self) -> ToolScenario {
        self.scenario
    }
    /// Returns the exact registered tool name.
    #[must_use]
    pub const fn tool_name(&self) -> &'static str {
        self.tool_name
    }
    /// Returns canonical structured arguments.
    #[must_use]
    pub const fn canonical_arguments(&self) -> &'static [u8] {
        self.canonical_arguments
    }
    /// Returns the exact output ceiling.
    #[must_use]
    pub const fn output_limit(&self) -> u64 {
        self.output_limit
    }
    /// Returns the exact call deadline relative to the fixture clock.
    #[must_use]
    pub const fn deadline_millis(&self) -> u64 {
        self.deadline_millis
    }
}

/// Adapter implemented by a production C4 router under conformance test.
pub trait ToolConformanceSubject: Send {
    /// Exercises one fixed request and returns direct observations rather than a claimed verdict.
    ///
    /// # Errors
    ///
    /// Returns [`ToolConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &ToolConformanceFixture,
    ) -> Result<ToolConformanceObservation, ToolConformanceError>;
}
