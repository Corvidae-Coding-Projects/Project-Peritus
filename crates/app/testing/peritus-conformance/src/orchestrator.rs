//! Runtime-neutral E0 actor orchestration conformance contract.

mod cases;

pub use cases::orchestrator_suite;

/// One independently exercised E0 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestratorScenario {
    /// Writer, gates, independent review, evaluation, and B0 observation complete in order.
    HappyPath,
    /// Blocking review findings take the sole fixer and fresh-revision loop.
    FixCycle,
    /// Actor, role, task, or work ownership drift is rejected.
    RoleDrift,
    /// Stale gate, review, evaluation, or kernel evidence cannot advance.
    StaleEvidence,
    /// Every material candidate change invalidates earlier quality facts.
    RevisionInvalidation,
    /// Independent cycle and revision bounds terminate rather than loop.
    LimitExhaustion,
    /// Pause preserves an exact reconciled resumable phase.
    PauseResume,
    /// Cancellation dominates every late child success.
    Cancellation,
    /// Journal replay and exact command retry recover without duplicate effects.
    Restart,
    /// Malformed or noncanonical protocol input is rejected while inert.
    MalformedProtocol,
    /// A panicking subject is contained by the conformance runner.
    PanicContainment,
    /// Teardown failure remains explicit and cannot manufacture a passing case.
    TeardownIsolation,
}

/// Stable terminal state observed from one E0 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestratorTerminal {
    /// A matching durable B0 acceptance event was observed.
    Accepted,
    /// The run was explicitly rejected.
    Rejected,
    /// An unrecoverable failure was recorded.
    Failed,
    /// A configured completion limit was exhausted.
    Exhausted,
    /// Human judgment or authority is required.
    NeedsHuman,
    /// Cancellation settled every active child.
    Cancelled,
}

/// Fixed bounds and revision marker supplied to one E0 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrchestratorConformanceFixture {
    scenario: OrchestratorScenario,
    maximum_revisions: u16,
    maximum_directives: u16,
    revision_marker: [u8; 32],
}

impl OrchestratorConformanceFixture {
    pub(crate) const fn new(scenario: OrchestratorScenario) -> Self {
        Self { scenario, maximum_revisions: 4, maximum_directives: 16, revision_marker: [0xe0; 32] }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> OrchestratorScenario {
        self.scenario
    }

    /// Returns the independent revision ceiling.
    #[must_use]
    pub const fn maximum_revisions(self) -> u16 {
        self.maximum_revisions
    }

    /// Returns the child-directive ceiling.
    #[must_use]
    pub const fn maximum_directives(self) -> u16 {
        self.maximum_directives
    }

    /// Returns the exact revision marker shared by the case.
    #[must_use]
    pub const fn revision_marker(self) -> [u8; 32] {
        self.revision_marker
    }
}

/// Direct facts observed while exercising one complete E0 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent lifecycle, durability, and acceptance facts must remain visible"
)]
pub struct OrchestratorConformanceObservation {
    /// Terminal E0 state.
    pub terminal: OrchestratorTerminal,
    /// Number of durable candidate revisions.
    pub revisions: u16,
    /// Number of durably created child directives.
    pub directives: u16,
    /// Writer, gate, review, fixer, and evaluation phases followed the closed order.
    pub phase_order_exact: bool,
    /// Every actor, role, task, work, run, attempt, and revision binding matched.
    pub ownership_exact: bool,
    /// A deliberately drifted actor, role, task, or work binding was rejected.
    pub ownership_drift_rejected: bool,
    /// Material revision advance invalidated every earlier quality fact.
    pub stale_evidence_rejected: bool,
    /// A fixer response returned through fresh gates and independent review.
    pub fix_cycle_exact: bool,
    /// Every configured loop and retained-state dimension stayed bounded.
    pub limits_enforced: bool,
    /// Pause retained and reconciled the exact resumable phase.
    pub pause_reconciled: bool,
    /// Cancellation dominated late completion and settled active children.
    pub cancellation_dominates: bool,
    /// Genesis replay reproduced the complete live state.
    pub replay_equivalent: bool,
    /// Exact command retry did not duplicate a directive or transition.
    pub idempotent_recovery: bool,
    /// Unknown, malformed, and trailing protocol bytes were rejected.
    pub malformed_rejected: bool,
    /// Acceptance followed B2 evaluation and a matching durable B0 event.
    pub b0_acceptance_observed: bool,
    /// Panic remained a typed case failure.
    pub panic_contained: bool,
    /// Teardown failure remained visible and non-passing.
    pub teardown_explicit: bool,
    /// No failure, stale fact, request, cancellation, or exhaustion implied acceptance.
    pub no_implicit_acceptance: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestratorConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by an E0 production subject or development bridge.
pub trait OrchestratorConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestratorConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &OrchestratorConformanceFixture,
    ) -> Result<OrchestratorConformanceObservation, OrchestratorConformanceError>;
}
