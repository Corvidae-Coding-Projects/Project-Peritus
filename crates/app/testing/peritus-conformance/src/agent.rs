//! Runtime-neutral D0 agent-loop conformance contract.

mod cases;

pub use cases::agent_suite;

/// One independently exercised D0 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentScenario {
    /// A complete inspect, edit, run, and test cycle reaches a completion proposal.
    InspectEditRunTest,
    /// Every active phase can pause and resume without losing its exact continuation.
    PauseResume,
    /// Cancellation settles owned model and tool work without implying success.
    Cancellation,
    /// Every durable event prefix replays to the same state and next effect.
    PrefixReplay,
    /// Role, memory, context, compaction, and render boundaries remain exact.
    ContextComposition,
    /// Fragmented, duplicate, malformed, and terminal provider events reduce truthfully.
    ProviderReduction,
    /// Retry and resumption occur only with the required provider protection.
    RetrySafety,
    /// Tool effects occur only after independent committed authority.
    ToolAuthorization,
    /// Poll, input, resize, signal, deadline, and cancellation remain owned and observable.
    ActiveToolControl,
    /// Concurrent inspection results return in their original proposal order.
    ParallelOrdering,
    /// Every authoritative and structural budget limit terminates explicitly.
    BudgetExhaustion,
    /// Completion requires settled work, current revisions, and fresh evidence.
    CompletionEligibility,
    /// Restart never redispatches an uncertain provider or tool effect.
    CrashNoRedispatch,
}

/// Stable terminal state directly observed from one exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTerminal {
    /// The inner turn produced one valid durable completion proposal.
    Completed,
    /// The inner turn failed explicitly.
    Failed,
    /// The inner turn was cancelled explicitly.
    Cancelled,
    /// The scenario intentionally remains resumable and nonterminal.
    Active,
}

/// Fixed bounds and identities supplied to one D0 conformance case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentConformanceFixture {
    scenario: AgentScenario,
    max_transitions: u64,
    max_model_attempts: u32,
    max_tool_calls: u32,
    parallel_limit: u32,
    revision_marker: [u8; 32],
}

impl AgentConformanceFixture {
    pub(crate) const fn new(scenario: AgentScenario) -> Self {
        Self {
            scenario,
            max_transitions: 512,
            max_model_attempts: 8,
            max_tool_calls: 32,
            parallel_limit: 4,
            revision_marker: [0xd0; 32],
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> AgentScenario {
        self.scenario
    }

    /// Returns the maximum accepted state transitions.
    #[must_use]
    pub const fn max_transitions(self) -> u64 {
        self.max_transitions
    }

    /// Returns the maximum accepted provider starts.
    #[must_use]
    pub const fn max_model_attempts(self) -> u32 {
        self.max_model_attempts
    }

    /// Returns the maximum accepted tool proposals.
    #[must_use]
    pub const fn max_tool_calls(self) -> u32 {
        self.max_tool_calls
    }

    /// Returns the maximum accepted concurrent inspection calls.
    #[must_use]
    pub const fn parallel_limit(self) -> u32 {
        self.parallel_limit
    }

    /// Returns the exact revision marker shared by the case.
    #[must_use]
    pub const fn revision_marker(self) -> [u8; 32] {
        self.revision_marker
    }
}

/// Direct facts observed while exercising one complete scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent direct observations must remain distinguishable to the conformance oracle"
)]
pub struct AgentConformanceObservation {
    /// Terminal state reached by the scenario.
    pub terminal: AgentTerminal,
    /// Number of accepted durable transitions.
    pub transitions: u64,
    /// Number of provider starts.
    pub model_attempts: u32,
    /// Number of tool proposals.
    pub tool_calls: u32,
    /// Greatest observed concurrent tool count.
    pub peak_parallel: u32,
    /// Every event-prefix replay matched live state and next effect.
    pub replay_equivalent: bool,
    /// No failure, cancellation, exhaustion, or uncertainty became success.
    pub no_implicit_success: bool,
    /// Every tool effect followed complete independent authority.
    pub authority_before_effect: bool,
    /// Every live provider/tool owner settled or remained explicitly owned.
    pub ownership_accounted: bool,
    /// Provider and tool results retained deterministic semantic ordering.
    pub stable_ordering: bool,
    /// Context, completion, and effects remained bound to the fixture revision.
    pub revision_exact: bool,
    /// Any completion proposal met all eligibility requirements.
    pub completion_eligible: bool,
    /// Restart did not recreate an effect whose outcome was uncertain.
    pub no_redispatch: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentConformanceError {
    /// The D0 boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a D0 production subject or its development bridge.
pub trait AgentConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations rather than a claimed verdict.
    ///
    /// # Errors
    ///
    /// Returns [`AgentConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &AgentConformanceFixture,
    ) -> Result<AgentConformanceObservation, AgentConformanceError>;
}
