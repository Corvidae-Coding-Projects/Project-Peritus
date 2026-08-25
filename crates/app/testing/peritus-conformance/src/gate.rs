//! Runtime-neutral D1 gate-engine conformance contract.

mod cases;

pub use cases::gate_suite;

/// One independently exercised D1 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateScenario {
    /// A real inspect/edit/run/test chain reaches a fresh passing aggregate.
    InspectEditRunTest,
    /// A failed prerequisite blocks every dependent without dispatch.
    FailedPrerequisite,
    /// Malformed parser output remains explicit non-success.
    MalformedParser,
    /// Evidence for another revision cannot satisfy the current run.
    StaleRevision,
    /// Cancellation settles active work and never implies success.
    Cancellation,
    /// Restart replays committed intent without repeating uncertain effects.
    CrashRecovery,
    /// Dispatch accepts only the exact clean immutable candidate snapshot.
    CleanSnapshot,
    /// Retry legality and the per-gate attempt ceiling are enforced.
    RetryBound,
    /// Passing evidence names complete finalized artifacts and provenance.
    ArtifactEvidence,
    /// Terminal aggregation is independent of observation arrival order.
    DeterministicAggregation,
}

/// Stable terminal state observed from one D1 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateTerminal {
    /// Every required gate passed with complete fresh evidence.
    Passed,
    /// A candidate or non-retryable infrastructure failure terminated the run.
    Failed,
    /// A failed prerequisite prevented dependent execution.
    Blocked,
    /// Cancellation completed after effect reconciliation.
    Cancelled,
    /// An effect remains explicitly indeterminate after recovery.
    Indeterminate,
}

/// Fixed bounds and revision marker supplied to one D1 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GateConformanceFixture {
    scenario: GateScenario,
    maximum_attempts: u16,
    maximum_dispatches: u16,
    revision_marker: [u8; 32],
}

impl GateConformanceFixture {
    pub(crate) const fn new(scenario: GateScenario) -> Self {
        Self { scenario, maximum_attempts: 3, maximum_dispatches: 8, revision_marker: [0xd1; 32] }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> GateScenario {
        self.scenario
    }
    /// Returns the per-gate attempt ceiling.
    #[must_use]
    pub const fn maximum_attempts(self) -> u16 {
        self.maximum_attempts
    }
    /// Returns the total dispatch ceiling for the fixture.
    #[must_use]
    pub const fn maximum_dispatches(self) -> u16 {
        self.maximum_dispatches
    }
    /// Returns the exact revision marker shared by the case.
    #[must_use]
    pub const fn revision_marker(self) -> [u8; 32] {
        self.revision_marker
    }
}

/// Direct facts observed while exercising one complete D1 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent gate truth and authority observations remain explicit"
)]
pub struct GateConformanceObservation {
    /// Terminal aggregate state.
    pub terminal: GateTerminal,
    /// Greatest attempt ordinal observed for any gate.
    pub peak_attempt: u16,
    /// Number of actual quality dispatches.
    pub dispatches: u16,
    /// Every dispatch followed the declared dependency order.
    pub dependencies_ordered: bool,
    /// Every result and artifact matched the requested revision.
    pub revision_exact: bool,
    /// Every effect targeted the exact clean immutable snapshot.
    pub clean_snapshot: bool,
    /// No malformed, partial, stale, cancelled, or failed input became pass.
    pub no_implicit_success: bool,
    /// Every effect crossed the existing authorized quality-tool path.
    pub authority_before_effect: bool,
    /// Event replay reproduced the live state and terminal aggregate.
    pub replay_equivalent: bool,
    /// Command retry and crash recovery were idempotent.
    pub idempotent_recovery: bool,
    /// Passing observations contained complete artifact/evidence provenance.
    pub evidence_complete: bool,
    /// Canonical aggregation was stable across permitted arrival orders.
    pub stable_aggregation: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a D1 production subject or development bridge.
pub trait GateConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`GateConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &GateConformanceFixture,
    ) -> Result<GateConformanceObservation, GateConformanceError>;
}
