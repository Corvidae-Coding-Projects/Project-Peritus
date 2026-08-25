//! Runtime-neutral D3 collaboration conformance contract.

mod cases;

pub use cases::collaboration_suite;

/// One independently exercised D3 collaboration behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollaborationScenario {
    /// Every task resolves through one acyclic parent chain to the root.
    CausalParentage,
    /// Offer, acceptance, activation, and owner/role binding are explicit.
    Delegation,
    /// Task depth, fan-out, and retained-record limits are enforced.
    BoundedGraph,
    /// Message ordinals and predecessor links are exact and contiguous.
    CausalMessages,
    /// All-required joins wait for every required successful child.
    AllRequiredJoin,
    /// Any-required joins use only declared required successful children.
    AnyRequiredJoin,
    /// Completion handoffs bind exact current artifact and revision evidence.
    ArtifactHandoff,
    /// Cancellation propagates through inactive and active descendants.
    CancellationTree,
    /// Restart and exact command retry reproduce durable state.
    Restart,
    /// Only a satisfied root and required joins can complete collaboration.
    TerminalTruth,
}

/// Stable terminal observed from one collaboration exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollaborationTerminal {
    /// Root and every required join completed successfully.
    Completed,
    /// A required task failed, was rejected, or was abandoned.
    Failed,
    /// A configured graph or record bound was exhausted.
    Exhausted,
    /// Human coordination is required to resolve incompatible outcomes.
    NeedsHuman,
    /// Root cancellation settled every descendant.
    Cancelled,
}

/// Fixed bounds supplied to one collaboration case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollaborationConformanceFixture {
    scenario: CollaborationScenario,
    maximum_tasks: u16,
    maximum_depth: u16,
    maximum_fanout: u16,
    maximum_messages: u16,
}

impl CollaborationConformanceFixture {
    pub(crate) const fn new(scenario: CollaborationScenario) -> Self {
        Self {
            scenario,
            maximum_tasks: 16,
            maximum_depth: 4,
            maximum_fanout: 4,
            maximum_messages: 32,
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> CollaborationScenario {
        self.scenario
    }
    /// Returns the task-count bound.
    #[must_use]
    pub const fn maximum_tasks(self) -> u16 {
        self.maximum_tasks
    }
    /// Returns the depth bound.
    #[must_use]
    pub const fn maximum_depth(self) -> u16 {
        self.maximum_depth
    }
    /// Returns the fan-out bound.
    #[must_use]
    pub const fn maximum_fanout(self) -> u16 {
        self.maximum_fanout
    }
    /// Returns the message-count bound.
    #[must_use]
    pub const fn maximum_messages(self) -> u16 {
        self.maximum_messages
    }
}

/// Direct facts observed while exercising one collaboration scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent causal, join, handoff, and recovery observations remain explicit"
)]
pub struct CollaborationConformanceObservation {
    /// Terminal collaboration state.
    pub terminal: CollaborationTerminal,
    /// Total retained tasks.
    pub tasks: u16,
    /// Greatest retained task depth.
    pub peak_depth: u16,
    /// Greatest direct child count.
    pub peak_fanout: u16,
    /// Total retained messages.
    pub messages: u16,
    /// Every task had one acyclic root/parent chain.
    pub parentage_valid: bool,
    /// Delegation owner, role, scheduler work, and lifecycle were exact.
    pub delegation_exact: bool,
    /// Graph and retained-record bounds were enforced before growth.
    pub bounds_enforced: bool,
    /// Message ordinals and predecessors were contiguous and task-local.
    pub messages_causal: bool,
    /// All-required joins waited for every required successful child.
    pub all_join_truthful: bool,
    /// Any-required joins used only declared required successful children.
    pub any_join_truthful: bool,
    /// Handoffs retained exact current revision and artifact evidence.
    pub handoff_exact: bool,
    /// Cancellation reached every descendant without resurrection.
    pub cancellation_complete: bool,
    /// Genesis replay reproduced complete live state.
    pub replay_equivalent: bool,
    /// Exact retry resolved without duplicate task, message, or event.
    pub idempotent_recovery: bool,
    /// No missing join, failure, cancellation, or exhaustion implied completion.
    pub no_implicit_success: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollaborationConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a D3 collaboration subject or development bridge.
pub trait CollaborationConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`CollaborationConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &CollaborationConformanceFixture,
    ) -> Result<CollaborationConformanceObservation, CollaborationConformanceError>;
}
