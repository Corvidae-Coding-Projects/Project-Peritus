//! Runtime-neutral D3 scheduler conformance contract.

mod cases;

pub use cases::scheduler_suite;

/// One independently exercised D3 scheduler behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerScenario {
    /// Feasible work is selected by the canonical priority/aging/order rule.
    DeterministicFairness,
    /// Global and worker reservations never exceed capacity.
    ResourceConservation,
    /// Work starts only after every dependency succeeds.
    DependencyReadiness,
    /// Worker ownership remains unique for each live dispatch.
    WorkerOwnership,
    /// Lost work follows its recorded retry/ambiguous/exhausted policy.
    WorkerLoss,
    /// Queue and attempt limits produce explicit backpressure or exhaustion.
    BoundedBackpressure,
    /// Pause and drain prevent new dispatch without losing ownership.
    PauseAndDrain,
    /// Cancellation propagates through queued and active descendants.
    CancellationTree,
    /// Restart and exact command retry reproduce durable state.
    Restart,
    /// Only complete successful work can produce scheduler completion.
    TerminalTruth,
}

/// Stable terminal observed from one scheduler exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerTerminal {
    /// Every admitted work item completed successfully.
    Completed,
    /// At least one item failed or became dependency-blocked.
    Failed,
    /// A resource, retry, or work bound was exhausted.
    Exhausted,
    /// An external effect remained explicitly ambiguous.
    Ambiguous,
    /// The scheduler and its active ownership were cancelled.
    Cancelled,
}

/// Fixed bounds supplied to a D3 scheduler case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerConformanceFixture {
    scenario: SchedulerScenario,
    maximum_work: u16,
    maximum_attempts: u16,
    maximum_bypass: u16,
}

impl SchedulerConformanceFixture {
    pub(crate) const fn new(scenario: SchedulerScenario) -> Self {
        Self { scenario, maximum_work: 16, maximum_attempts: 3, maximum_bypass: 4 }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> SchedulerScenario {
        self.scenario
    }
    /// Returns the admitted work bound.
    #[must_use]
    pub const fn maximum_work(self) -> u16 {
        self.maximum_work
    }
    /// Returns the per-work attempt bound.
    #[must_use]
    pub const fn maximum_attempts(self) -> u16 {
        self.maximum_attempts
    }
    /// Returns the maximum feasible bypass count.
    #[must_use]
    pub const fn maximum_bypass(self) -> u16 {
        self.maximum_bypass
    }
}

/// Direct facts observed while exercising one complete scheduler scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent scheduling safety and liveness observations remain explicit"
)]
pub struct SchedulerConformanceObservation {
    /// Terminal scheduler state.
    pub terminal: SchedulerTerminal,
    /// Total admitted work records.
    pub work: u16,
    /// Greatest attempt ordinal.
    pub peak_attempt: u16,
    /// Greatest feasible bypass count.
    pub peak_bypass: u16,
    /// Selection matched the canonical reference order.
    pub selection_deterministic: bool,
    /// Every live allocation remained within global and worker capacity.
    pub resources_conserved: bool,
    /// No work started before all dependencies succeeded.
    pub dependencies_satisfied: bool,
    /// Every live dispatch had exactly one owner and work attempt.
    pub ownership_unique: bool,
    /// Worker loss preserved retry and ambiguity truth.
    pub loss_truthful: bool,
    /// Admission and attempt bounds were enforced before growth.
    pub backpressure_bounded: bool,
    /// Pause/drain prevented new dispatch while retaining ownership.
    pub pause_respected: bool,
    /// Cancellation reached every descendant without resurrection.
    pub cancellation_complete: bool,
    /// Genesis replay reproduced the complete live state.
    pub replay_equivalent: bool,
    /// Exact retry resolved without duplicate reservation or event.
    pub idempotent_recovery: bool,
    /// No failure, cancellation, ambiguity, or exhaustion implied success.
    pub no_implicit_success: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a D3 scheduler subject or development bridge.
pub trait SchedulerConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &SchedulerConformanceFixture,
    ) -> Result<SchedulerConformanceObservation, SchedulerConformanceError>;
}
