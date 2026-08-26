//! Runtime-neutral E2 debugger conformance contract.

mod cases;

pub use cases::debugger_suite;

/// One independently exercised E2 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerScenario {
    /// Immutable subject bindings select only exact C7/C0-backed evidence.
    EvidenceSelection,
    /// Selected events form a canonical bounded causal timeline.
    TimelineConstruction,
    /// Every normalized outcome and cause uses the closed taxonomy.
    TaxonomyCompleteness,
    /// Claims cite only selected observations and bounded artifact ranges.
    CitationContainment,
    /// Invalid or authority-bearing model output is rejected as a whole.
    ModelOutputRejection,
    /// Cross-run patterns are deterministic regardless of input ordering.
    DeterministicClustering,
    /// Journal replay and exact retry reproduce state without duplicate effects.
    DurableReplay,
    /// Cancellation is durable, terminal, and cannot become completion.
    Cancellation,
    /// Unknown, noncanonical, and trailing wire input remains inert.
    MalformedInput,
    /// Default reports, failures, and diagnostics contain no sensitive canary.
    Redaction,
    /// Independent selection, analysis, model, report, and state limits hold.
    BoundedResources,
    /// A panicking subject remains a typed failed conformance case.
    PanicContainment,
    /// Teardown failure stays visible and cannot manufacture a passing suite.
    TeardownIsolation,
}

/// Stable terminal observed from one E2 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerTerminal {
    /// A validated diagnostic report completed.
    Completed,
    /// Invalid input or analysis output was rejected without report publication.
    Rejected,
    /// The job settled as durably cancelled.
    Cancelled,
}

/// Fixed realistic bounds supplied to one E2 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebuggerConformanceFixture {
    scenario: DebuggerScenario,
    maximum_selected_events: u32,
    maximum_timeline_entries: u32,
    maximum_causes: u16,
    maximum_patterns: u16,
    canary: &'static str,
}

impl DebuggerConformanceFixture {
    pub(crate) const fn new(scenario: DebuggerScenario) -> Self {
        Self {
            scenario,
            maximum_selected_events: 32,
            maximum_timeline_entries: 64,
            maximum_causes: 16,
            maximum_patterns: 8,
            canary: "peritus-e2-sensitive-canary",
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> DebuggerScenario {
        self.scenario
    }

    /// Returns the selected-event ceiling.
    #[must_use]
    pub const fn maximum_selected_events(self) -> u32 {
        self.maximum_selected_events
    }

    /// Returns the timeline-entry ceiling.
    #[must_use]
    pub const fn maximum_timeline_entries(self) -> u32 {
        self.maximum_timeline_entries
    }

    /// Returns the cause ceiling.
    #[must_use]
    pub const fn maximum_causes(self) -> u16 {
        self.maximum_causes
    }

    /// Returns the pattern ceiling.
    #[must_use]
    pub const fn maximum_patterns(self) -> u16 {
        self.maximum_patterns
    }

    /// Returns the sensitive canary excluded from every default surface.
    #[must_use]
    pub const fn canary(self) -> &'static str {
        self.canary
    }
}

/// Direct observations from one complete E2 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent diagnostic, durability, redaction, and authority facts remain explicit"
)]
pub struct DebuggerConformanceObservation {
    /// Terminal outcome.
    pub terminal: DebuggerTerminal,
    /// Events retained by the immutable selection manifest.
    pub selected_events: u32,
    /// Entries retained by all constructed timelines.
    pub timeline_entries: u32,
    /// Root-cause candidates retained by the report.
    pub causes: u16,
    /// Cross-run patterns retained by the report.
    pub patterns: u16,
    /// Selection matched one frozen subject and checked C0/C7 evidence exactly.
    pub selection_exact: bool,
    /// Causal timeline order and outcome normalization were exact.
    pub timeline_exact: bool,
    /// Every represented failure used the complete closed taxonomy.
    pub taxonomy_complete: bool,
    /// Every claim and artifact range remained inside selected evidence.
    pub citations_contained: bool,
    /// Invalid model output was rejected without changing deterministic findings.
    pub model_rejection_exact: bool,
    /// Pattern fingerprints and membership were canonical and deterministic.
    pub clustering_deterministic: bool,
    /// Replay and exact retry reproduced state without duplicate effects.
    pub replay_equivalent: bool,
    /// Cancellation was durable, terminal, and idempotent.
    pub cancellation_durable: bool,
    /// Malformed protocol input remained inert and rejected.
    pub malformed_rejected: bool,
    /// Sensitive canaries were absent from default reports and diagnostics.
    pub redaction_safe: bool,
    /// Every independent configured limit was enforced without silent truncation.
    pub bounds_enforced: bool,
    /// Panic was contained as a case failure.
    pub panic_contained: bool,
    /// Teardown failure remained explicit and non-passing.
    pub teardown_explicit: bool,
    /// No diagnostic output represented mutation, acceptance, evaluation, or promotion authority.
    pub non_authoritative: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by an E2 production subject or development bridge.
pub trait DebuggerConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &DebuggerConformanceFixture,
    ) -> Result<DebuggerConformanceObservation, DebuggerConformanceError>;
}
