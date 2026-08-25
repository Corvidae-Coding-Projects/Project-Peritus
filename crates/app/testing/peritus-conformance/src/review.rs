//! Runtime-neutral D2 review-engine conformance contract.

mod cases;

pub use cases::review_suite;

/// One independently exercised D2 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewScenario {
    /// Assignment, submission, disposition, and finalization follow the closed lifecycle.
    Lifecycle,
    /// Required review count and category coverage are enforced independently.
    Quorum,
    /// Every configured reviewer-independence dimension is enforced.
    Independence,
    /// Duplicate reconciliation retains every source and evidence reference.
    Reconciliation,
    /// Evidence for an earlier exact revision becomes non-authoritative.
    StaleRevision,
    /// A fixer response closes only after current reviewer confirmation.
    Resolution,
    /// A waiver closes only from an external current authority observation.
    Waiver,
    /// Restart and command retry reproduce the exact durable state.
    Restart,
    /// Repeated or non-improving findings terminate truthfully.
    Oscillation,
    /// Malformed structured submissions cannot count or imply success.
    MalformedSubmission,
}

/// Stable terminal state observed from one D2 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewTerminal {
    /// Review evidence is complete; this is not overall run acceptance.
    Completed,
    /// Human judgment or authority is required.
    NeedsHuman,
    /// An unrecoverable review-engine failure was recorded.
    Failed,
    /// Review work was explicitly cancelled.
    Cancelled,
}

/// Fixed bounds and revision marker supplied to one D2 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewConformanceFixture {
    scenario: ReviewScenario,
    maximum_cycles: u16,
    maximum_findings: u16,
    revision_marker: [u8; 32],
}

impl ReviewConformanceFixture {
    pub(crate) const fn new(scenario: ReviewScenario) -> Self {
        Self { scenario, maximum_cycles: 4, maximum_findings: 64, revision_marker: [0xd2; 32] }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> ReviewScenario {
        self.scenario
    }
    /// Returns the review-cycle ceiling.
    #[must_use]
    pub const fn maximum_cycles(self) -> u16 {
        self.maximum_cycles
    }
    /// Returns the finding-count ceiling.
    #[must_use]
    pub const fn maximum_findings(self) -> u16 {
        self.maximum_findings
    }
    /// Returns the exact revision marker shared by the case.
    #[must_use]
    pub const fn revision_marker(self) -> [u8; 32] {
        self.revision_marker
    }
}

/// Direct facts observed while exercising one complete D2 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent review truth, provenance, and authority observations remain explicit"
)]
pub struct ReviewConformanceObservation {
    /// Terminal D2 state.
    pub terminal: ReviewTerminal,
    /// Number of durable reviewer cycles.
    pub cycles: u16,
    /// Number of current canonical findings.
    pub findings: u16,
    /// Every authoritative observation names the exact current revision.
    pub revision_exact: bool,
    /// Required review count and category coverage both passed.
    pub quorum_complete: bool,
    /// Every enabled independence dimension passed separately.
    pub independence_complete: bool,
    /// Reconciled duplicates retained every source and evidence reference.
    pub provenance_retained: bool,
    /// Every current finding remains open or has one permitted current closure.
    pub findings_conserved: bool,
    /// Resolution, invalidation, and supersession require reviewer confirmation.
    pub reviewer_confirmed: bool,
    /// Waiver closure came only from an external authorized observation.
    pub waiver_external: bool,
    /// Stale evidence was excluded from quorum and conservation decisions.
    pub stale_rejected: bool,
    /// Genesis replay reproduced the complete live state.
    pub replay_equivalent: bool,
    /// Exact command retry resolved without a duplicate transition.
    pub idempotent_recovery: bool,
    /// Repetition, stagnation, and exhaustion produced non-success.
    pub oscillation_truthful: bool,
    /// Malformed structured data was rejected before quorum accounting.
    pub malformed_rejected: bool,
    /// No failure, cancellation, stale record, malformed input, or exhaustion implied completion.
    pub no_implicit_success: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a D2 production subject or development bridge.
pub trait ReviewConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &ReviewConformanceFixture,
    ) -> Result<ReviewConformanceObservation, ReviewConformanceError>;
}
