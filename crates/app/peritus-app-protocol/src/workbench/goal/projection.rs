//! Cumulative role accounting and the complete current goal projection.

use super::{
    AppProtocolError, ControlOperationId, MAX_WORKBENCH_GOAL_CRITERIA, RunId, WorkbenchGoalBudget,
    WorkbenchGoalCriterion, WorkbenchGoalPauseMode, WorkbenchGoalState, WorkbenchInputText,
    WorkbenchQuery, invalid,
};

/// Stable role ordering used by usage projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchGoalRole {
    /// Designer, conversational, and writer work.
    Writer,
    /// Independent review work.
    Reviewer,
    /// Review-remediation work.
    Fixer,
}

/// Per-role reservations and provider usage. Availability flags govern zero-valued counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalRoleUsage {
    role: WorkbenchGoalRole,
    requests: u32,
    completed_requests: u32,
    tool_calls: u32,
    total_tokens: Option<u64>,
    provider_cost_microunits: Option<u64>,
}

impl WorkbenchGoalRoleUsage {
    /// Creates one truthful role row; absent usage must remain `None` rather than zero.
    #[must_use]
    pub const fn new(
        role: WorkbenchGoalRole,
        requests: u32,
        completed_requests: u32,
        tool_calls: u32,
        total_tokens: Option<u64>,
        provider_cost_microunits: Option<u64>,
    ) -> Self {
        Self {
            role,
            requests,
            completed_requests,
            tool_calls,
            total_tokens,
            provider_cost_microunits,
        }
    }
    /// Locally attributed execution role.
    #[must_use]
    pub const fn role(self) -> WorkbenchGoalRole {
        self.role
    }
    /// Requests durably reserved before provider admission.
    #[must_use]
    pub const fn requests(self) -> u32 {
        self.requests
    }
    /// Requests with a conclusive provider boundary.
    #[must_use]
    pub const fn completed_requests(self) -> u32 {
        self.completed_requests
    }
    /// Tool operations durably reserved before execution.
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Total tokens only when every role request supplied usable counters.
    #[must_use]
    pub const fn total_tokens(self) -> Option<u64> {
        self.total_tokens
    }
    /// Provider-estimated microunits only when explicitly reported.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> Option<u64> {
        self.provider_cost_microunits
    }
}

/// Aggregate cumulative goal accounting, retained across attempts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalUsage {
    roles: [WorkbenchGoalRoleUsage; 3],
    active_millis: u64,
    wall_millis: u64,
    retries: u32,
    provider_failovers: u32,
    compactions: u32,
    workspace_bytes: u64,
    workspace_growth_bytes: u64,
    peak_rss_bytes: u64,
}

impl WorkbenchGoalUsage {
    /// Constructs the complete cumulative public accounting projection.
    #[allow(clippy::too_many_arguments, reason = "public accounting fields remain explicit")]
    #[must_use]
    pub const fn new(
        roles: [WorkbenchGoalRoleUsage; 3],
        active_millis: u64,
        wall_millis: u64,
        retries: u32,
        provider_failovers: u32,
        compactions: u32,
        workspace_bytes: u64,
        workspace_growth_bytes: u64,
        peak_rss_bytes: u64,
    ) -> Self {
        Self {
            roles,
            active_millis,
            wall_millis,
            retries,
            provider_failovers,
            compactions,
            workspace_bytes,
            workspace_growth_bytes,
            peak_rss_bytes,
        }
    }
    /// Writer, reviewer, and fixer rows in canonical order.
    #[must_use]
    pub const fn roles(&self) -> &[WorkbenchGoalRoleUsage; 3] {
        &self.roles
    }
    /// Active runner milliseconds, excluding deliberate pause intervals.
    #[must_use]
    pub const fn active_millis(&self) -> u64 {
        self.active_millis
    }
    /// Wall time since confirmation, including pause intervals.
    #[must_use]
    pub const fn wall_millis(&self) -> u64 {
        self.wall_millis
    }
    /// Checked provider and role retries across attempts.
    #[must_use]
    pub const fn retries(&self) -> u32 {
        self.retries
    }
    /// Explicit provider failovers across attempts.
    #[must_use]
    pub const fn provider_failovers(&self) -> u32 {
        self.provider_failovers
    }
    /// Deterministic context compactions across attempts.
    #[must_use]
    pub const fn compactions(&self) -> u32 {
        self.compactions
    }
    /// Latest observed workspace bytes.
    #[must_use]
    pub const fn workspace_bytes(&self) -> u64 {
        self.workspace_bytes
    }
    /// Maximum observed workspace growth.
    #[must_use]
    pub const fn workspace_growth_bytes(&self) -> u64 {
        self.workspace_growth_bytes
    }
    /// Maximum observed process resident memory.
    #[must_use]
    pub const fn peak_rss_bytes(&self) -> u64 {
        self.peak_rss_bytes
    }
    /// Total reserved provider requests.
    #[must_use]
    pub fn requests(&self) -> u32 {
        self.roles.iter().fold(0, |sum, row| sum.saturating_add(row.requests))
    }
    /// Total reserved tool operations.
    #[must_use]
    pub fn tool_calls(&self) -> u32 {
        self.roles.iter().fold(0, |sum, row| sum.saturating_add(row.tool_calls))
    }
    /// Aggregate total tokens, or `None` when any reserved request lacks reporting.
    #[must_use]
    pub fn total_tokens(&self) -> Option<u64> {
        self.roles.iter().try_fold(0_u64, |sum, row| sum.checked_add(row.total_tokens?))
    }
    /// Aggregate estimated microunits, or `None` when any request lacks cost reporting.
    #[must_use]
    pub fn provider_cost_microunits(&self) -> Option<u64> {
        self.roles.iter().try_fold(0_u64, |sum, row| sum.checked_add(row.provider_cost_microunits?))
    }
}

/// Complete current goal projection used by `/goal`, `/usage`, and `/budget`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalSnapshot {
    query: WorkbenchQuery,
    aggregate_revision: u64,
    goal: ControlOperationId,
    run: RunId,
    objective: WorkbenchInputText,
    state: WorkbenchGoalState,
    reason: String,
    user_revision: u64,
    attempt: u32,
    restart_eligible: bool,
    pause_mode: Option<WorkbenchGoalPauseMode>,
    criteria: Vec<WorkbenchGoalCriterion>,
    budget: WorkbenchGoalBudget,
    usage: WorkbenchGoalUsage,
}

impl WorkbenchGoalSnapshot {
    /// Constructs a checked complete goal snapshot.
    ///
    /// # Errors
    /// Rejects invalid revisions, reason text, criterion counts, or pause-state mismatch.
    #[allow(clippy::too_many_arguments, reason = "goal projection is an explicit protocol record")]
    pub fn new(
        query: WorkbenchQuery,
        aggregate_revision: u64,
        goal: ControlOperationId,
        run: RunId,
        objective: WorkbenchInputText,
        state: WorkbenchGoalState,
        reason: String,
        user_revision: u64,
        attempt: u32,
        restart_eligible: bool,
        pause_mode: Option<WorkbenchGoalPauseMode>,
        criteria: Vec<WorkbenchGoalCriterion>,
        budget: WorkbenchGoalBudget,
        usage: WorkbenchGoalUsage,
    ) -> Result<Self, AppProtocolError> {
        if aggregate_revision == 0
            || user_revision == 0
            || attempt == 0
            || reason.trim().is_empty()
            || reason.len() > 512
            || reason.chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            || criteria.is_empty()
            || criteria.len() > MAX_WORKBENCH_GOAL_CRITERIA
            || (state == WorkbenchGoalState::Pausing) != pause_mode.is_some()
        {
            return Err(invalid());
        }
        Ok(Self {
            query,
            aggregate_revision,
            goal,
            run,
            objective,
            state,
            reason,
            user_revision,
            attempt,
            restart_eligible,
            pause_mode,
            criteria,
            budget,
            usage,
        })
    }
    /// Exact conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Current aggregate revision used for subsequent control intents.
    #[must_use]
    pub const fn aggregate_revision(&self) -> u64 {
        self.aggregate_revision
    }
    /// Original goal/start operation identity.
    #[must_use]
    pub const fn goal(&self) -> ControlOperationId {
        self.goal
    }
    /// Existing product-run identity retained across attempts.
    #[must_use]
    pub const fn run(&self) -> RunId {
        self.run
    }
    /// Exact confirmed objective.
    #[must_use]
    pub const fn objective(&self) -> &WorkbenchInputText {
        &self.objective
    }
    /// Current durable goal state.
    #[must_use]
    pub const fn state(&self) -> WorkbenchGoalState {
        self.state
    }
    /// Exact last transition reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    /// User-operation revision, independent of host accounting events.
    #[must_use]
    pub const fn user_revision(&self) -> u64 {
        self.user_revision
    }
    /// Current one-based runner attempt.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }
    /// Whether crash recovery may revalidate and continue automatically.
    #[must_use]
    pub const fn restart_eligible(&self) -> bool {
        self.restart_eligible
    }
    /// Pending safe-boundary mode while state is pausing.
    #[must_use]
    pub const fn pause_mode(&self) -> Option<WorkbenchGoalPauseMode> {
        self.pause_mode
    }
    /// Criteria and current evidence states.
    #[must_use]
    pub fn criteria(&self) -> &[WorkbenchGoalCriterion] {
        &self.criteria
    }
    /// Current cumulative user limits.
    #[must_use]
    pub const fn budget(&self) -> WorkbenchGoalBudget {
        self.budget
    }
    /// Cumulative usage across all attempts.
    #[must_use]
    pub const fn usage(&self) -> &WorkbenchGoalUsage {
        &self.usage
    }
}
