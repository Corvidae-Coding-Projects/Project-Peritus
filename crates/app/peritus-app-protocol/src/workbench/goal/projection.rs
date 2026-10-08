//! Cumulative role accounting and the complete current goal projection.

use super::{
    AppProtocolError, ControlOperationId, RunId, WorkbenchGoalCriterion, WorkbenchGoalPauseMode,
    WorkbenchGoalState, WorkbenchGoalText, WorkbenchQuery, invalid,
};
use std::cmp::Ordering;

/// Exact cumulative goal counter stored as canonical little-endian `u64` limbs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkbenchGoalCounter<const LEGACY_BITS: u8> {
    segments: Vec<u64>,
}

/// Request/tool/retry count with the historical `u32` wire frontier.
pub type WorkbenchGoalCount = WorkbenchGoalCounter<32>;
/// Token/time/cost amount with the historical `u64` wire frontier.
pub type WorkbenchGoalAmount = WorkbenchGoalCounter<64>;

impl<const LEGACY_BITS: u8> WorkbenchGoalCounter<LEGACY_BITS> {
    fn legacy_max() -> Option<u64> {
        match LEGACY_BITS {
            32 => Some(u64::from(u32::MAX)),
            64 => Some(u64::MAX),
            _ => None,
        }
    }

    fn from_u64(value: u64) -> Self {
        Self { segments: if value == 0 { Vec::new() } else { vec![value] } }
    }

    /// Reconstructs an exact counter from canonical little-endian limbs.
    ///
    /// # Errors
    /// Rejects unsupported legacy widths and a redundant most-significant zero limb.
    pub fn from_segments(segments: Vec<u64>) -> Result<Self, AppProtocolError> {
        if Self::legacy_max().is_none() || segments.last() == Some(&0) {
            return Err(invalid());
        }
        Ok(Self { segments })
    }

    /// Borrows canonical little-endian limbs; an empty slice represents zero.
    #[must_use]
    pub fn segments(&self) -> &[u64] {
        &self.segments
    }

    /// Returns the historical scalar representation when it remains exact.
    #[must_use]
    pub fn legacy_value(&self) -> Option<u64> {
        let value = match self.segments.as_slice() {
            [] => 0,
            [value] => *value,
            _ => return None,
        };
        (value <= Self::legacy_max()?).then_some(value)
    }

    fn add_counter(&mut self, value: &Self) {
        if self.segments.len() < value.segments.len() {
            self.segments.resize(value.segments.len(), 0);
        }
        let mut carry = false;
        for index in 0..self.segments.len() {
            let addend = value.segments.get(index).copied().unwrap_or(0);
            let (partial, first) = self.segments[index].overflowing_add(addend);
            let (sum, second) = partial.overflowing_add(u64::from(carry));
            self.segments[index] = sum;
            carry = first || second;
        }
        if carry {
            self.segments.push(1);
        }
    }
}

impl WorkbenchGoalCount {
    /// Creates a count from its historical scalar form.
    #[must_use]
    pub fn from_u32(value: u32) -> Self {
        Self::from_u64(u64::from(value))
    }
}

impl WorkbenchGoalAmount {
    /// Creates an amount from its historical scalar form.
    #[must_use]
    pub fn from_u64_value(value: u64) -> Self {
        Self::from_u64(value)
    }
}

impl<const LEGACY_BITS: u8> Ord for WorkbenchGoalCounter<LEGACY_BITS> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.segments
            .len()
            .cmp(&other.segments.len())
            .then_with(|| self.segments.iter().rev().cmp(other.segments.iter().rev()))
    }
}

impl<const LEGACY_BITS: u8> PartialOrd for WorkbenchGoalCounter<LEGACY_BITS> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const LEGACY_BITS: u8> PartialEq<u64> for WorkbenchGoalCounter<LEGACY_BITS> {
    fn eq(&self, other: &u64) -> bool {
        match self.segments.as_slice() {
            [] => *other == 0,
            [value] => value == other,
            _ => false,
        }
    }
}

impl<const LEGACY_BITS: u8> std::fmt::Display for WorkbenchGoalCounter<LEGACY_BITS> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const BASE: u128 = 1_000_000_000;
        if self.segments.is_empty() {
            return f.write_str("0");
        }
        let mut decimal = vec![0_u32];
        for segment in self.segments.iter().rev() {
            let mut carry = u128::from(*segment);
            for chunk in &mut decimal {
                let value = u128::from(*chunk) * (u128::from(u64::MAX) + 1) + carry;
                *chunk = u32::try_from(value % BASE).map_err(|_| std::fmt::Error)?;
                carry = value / BASE;
            }
            while carry != 0 {
                decimal.push(u32::try_from(carry % BASE).map_err(|_| std::fmt::Error)?);
                carry /= BASE;
            }
        }
        let mut chunks = decimal.iter().rev();
        let Some(first) = chunks.next() else { return f.write_str("0") };
        write!(f, "{first}")?;
        for chunk in chunks {
            write!(f, "{chunk:09}")?;
        }
        Ok(())
    }
}

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalRoleUsage {
    role: WorkbenchGoalRole,
    requests: WorkbenchGoalCount,
    completed_requests: WorkbenchGoalCount,
    tool_calls: WorkbenchGoalCount,
    total_tokens: Option<WorkbenchGoalAmount>,
    provider_cost_microunits: Option<WorkbenchGoalAmount>,
}

impl WorkbenchGoalRoleUsage {
    /// Creates one truthful role row; absent usage must remain `None` rather than zero.
    #[must_use]
    pub fn new(
        role: WorkbenchGoalRole,
        requests: u32,
        completed_requests: u32,
        tool_calls: u32,
        total_tokens: Option<u64>,
        provider_cost_microunits: Option<u64>,
    ) -> Self {
        Self::new_exact(
            role,
            WorkbenchGoalCount::from_u32(requests),
            WorkbenchGoalCount::from_u32(completed_requests),
            WorkbenchGoalCount::from_u32(tool_calls),
            total_tokens.map(WorkbenchGoalAmount::from_u64_value),
            provider_cost_microunits.map(WorkbenchGoalAmount::from_u64_value),
        )
    }
    /// Creates one role row from exact segmented counters.
    #[must_use]
    pub const fn new_exact(
        role: WorkbenchGoalRole,
        requests: WorkbenchGoalCount,
        completed_requests: WorkbenchGoalCount,
        tool_calls: WorkbenchGoalCount,
        total_tokens: Option<WorkbenchGoalAmount>,
        provider_cost_microunits: Option<WorkbenchGoalAmount>,
    ) -> Self {
        Self { role, requests, completed_requests, tool_calls, total_tokens, provider_cost_microunits }
    }
    /// Locally attributed execution role.
    #[must_use]
    pub const fn role(&self) -> WorkbenchGoalRole {
        self.role
    }
    /// Requests durably reserved before provider admission.
    #[must_use]
    pub fn requests(&self) -> WorkbenchGoalCount {
        self.requests.clone()
    }
    /// Requests with a conclusive provider boundary.
    #[must_use]
    pub fn completed_requests(&self) -> WorkbenchGoalCount {
        self.completed_requests.clone()
    }
    /// Tool operations durably reserved before execution.
    #[must_use]
    pub fn tool_calls(&self) -> WorkbenchGoalCount {
        self.tool_calls.clone()
    }
    /// Total tokens only when every role request supplied usable counters.
    #[must_use]
    pub fn total_tokens(&self) -> Option<WorkbenchGoalAmount> {
        self.total_tokens.clone()
    }
    /// Provider-estimated microunits only when explicitly reported.
    #[must_use]
    pub fn provider_cost_microunits(&self) -> Option<WorkbenchGoalAmount> {
        self.provider_cost_microunits.clone()
    }

    fn legacy_wire_representable(&self) -> bool {
        self.requests.legacy_value().is_some()
            && self.completed_requests.legacy_value().is_some()
            && self.tool_calls.legacy_value().is_some()
            && self.total_tokens.as_ref().is_none_or(|value| value.legacy_value().is_some())
            && self
                .provider_cost_microunits
                .as_ref()
                .is_none_or(|value| value.legacy_value().is_some())
    }
}

/// Aggregate cumulative goal accounting, retained across attempts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalUsage {
    roles: [WorkbenchGoalRoleUsage; 3],
    active_millis: WorkbenchGoalAmount,
    wall_millis: u64,
    retries: WorkbenchGoalCount,
    provider_failovers: WorkbenchGoalCount,
    compactions: WorkbenchGoalCount,
    workspace_bytes: u64,
    workspace_growth_bytes: u64,
    peak_rss_bytes: u64,
}

impl WorkbenchGoalUsage {
    /// Constructs the complete cumulative public accounting projection.
    #[allow(clippy::too_many_arguments, reason = "public accounting fields remain explicit")]
    #[must_use]
    pub fn new(
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
        Self::new_exact(
            roles,
            WorkbenchGoalAmount::from_u64_value(active_millis),
            wall_millis,
            WorkbenchGoalCount::from_u32(retries),
            WorkbenchGoalCount::from_u32(provider_failovers),
            WorkbenchGoalCount::from_u32(compactions),
            workspace_bytes,
            workspace_growth_bytes,
            peak_rss_bytes,
        )
    }
    /// Constructs accounting from exact cumulative counters.
    #[allow(clippy::too_many_arguments, reason = "public accounting fields remain explicit")]
    #[must_use]
    pub const fn new_exact(
        roles: [WorkbenchGoalRoleUsage; 3],
        active_millis: WorkbenchGoalAmount,
        wall_millis: u64,
        retries: WorkbenchGoalCount,
        provider_failovers: WorkbenchGoalCount,
        compactions: WorkbenchGoalCount,
        workspace_bytes: u64,
        workspace_growth_bytes: u64,
        peak_rss_bytes: u64,
    ) -> Self {
        Self { roles, active_millis, wall_millis, retries, provider_failovers, compactions, workspace_bytes, workspace_growth_bytes, peak_rss_bytes }
    }
    /// Writer, reviewer, and fixer rows in canonical order.
    #[must_use]
    pub const fn roles(&self) -> &[WorkbenchGoalRoleUsage; 3] {
        &self.roles
    }
    /// Active runner milliseconds, excluding deliberate pause intervals.
    #[must_use]
    pub fn active_millis(&self) -> WorkbenchGoalAmount {
        self.active_millis.clone()
    }
    /// Wall time since confirmation, including pause intervals.
    #[must_use]
    pub const fn wall_millis(&self) -> u64 {
        self.wall_millis
    }
    /// Checked provider and role retries across attempts.
    #[must_use]
    pub fn retries(&self) -> WorkbenchGoalCount {
        self.retries.clone()
    }
    /// Explicit provider failovers across attempts.
    #[must_use]
    pub fn provider_failovers(&self) -> WorkbenchGoalCount {
        self.provider_failovers.clone()
    }
    /// Deterministic context compactions across attempts.
    #[must_use]
    pub fn compactions(&self) -> WorkbenchGoalCount {
        self.compactions.clone()
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
    pub fn requests(&self) -> WorkbenchGoalCount {
        self.roles.iter().fold(WorkbenchGoalCount::default(), |mut sum, row| {
            sum.add_counter(&row.requests);
            sum
        })
    }
    /// Total reserved tool operations.
    #[must_use]
    pub fn tool_calls(&self) -> WorkbenchGoalCount {
        self.roles.iter().fold(WorkbenchGoalCount::default(), |mut sum, row| {
            sum.add_counter(&row.tool_calls);
            sum
        })
    }
    /// Aggregate total tokens, or `None` when any reserved request lacks reporting.
    #[must_use]
    pub fn total_tokens(&self) -> Option<WorkbenchGoalAmount> {
        self.roles.iter().try_fold(WorkbenchGoalAmount::default(), |mut sum, row| {
            sum.add_counter(row.total_tokens.as_ref()?);
            Some(sum)
        })
    }
    /// Aggregate estimated microunits, or `None` when any request lacks cost reporting.
    #[must_use]
    pub fn provider_cost_microunits(&self) -> Option<WorkbenchGoalAmount> {
        self.roles.iter().try_fold(WorkbenchGoalAmount::default(), |mut sum, row| {
            sum.add_counter(row.provider_cost_microunits.as_ref()?);
            Some(sum)
        })
    }

    fn legacy_wire_representable(&self) -> bool {
        self.roles.iter().all(WorkbenchGoalRoleUsage::legacy_wire_representable)
            && self.active_millis.legacy_value().is_some()
            && self.retries.legacy_value().is_some()
            && self.provider_failovers.legacy_value().is_some()
            && self.compactions.legacy_value().is_some()
    }
}

/// Complete current goal projection used by `/goal` and `/usage`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGoalSnapshot {
    query: WorkbenchQuery,
    aggregate_revision: u64,
    goal: ControlOperationId,
    run: RunId,
    objective: WorkbenchGoalText,
    state: WorkbenchGoalState,
    reason: String,
    user_revision: u64,
    attempt: u32,
    restart_eligible: bool,
    pause_mode: Option<WorkbenchGoalPauseMode>,
    criteria: Vec<WorkbenchGoalCriterion>,
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
        objective: impl Into<WorkbenchGoalText>,
        state: WorkbenchGoalState,
        reason: String,
        user_revision: u64,
        attempt: u32,
        restart_eligible: bool,
        pause_mode: Option<WorkbenchGoalPauseMode>,
        criteria: Vec<WorkbenchGoalCriterion>,
        usage: WorkbenchGoalUsage,
    ) -> Result<Self, AppProtocolError> {
        if aggregate_revision == 0
            || user_revision == 0
            || attempt == 0
            || reason.trim().is_empty()
            || reason.chars().any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
            || criteria.is_empty()
            || (state == WorkbenchGoalState::Pausing) != pause_mode.is_some()
        {
            return Err(invalid());
        }
        Ok(Self {
            query,
            aggregate_revision,
            goal,
            run,
            objective: objective.into(),
            state,
            reason,
            user_revision,
            attempt,
            restart_eligible,
            pause_mode,
            criteria,
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
    pub const fn objective(&self) -> &WorkbenchGoalText {
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
    /// Cumulative usage across all attempts.
    #[must_use]
    pub const fn usage(&self) -> &WorkbenchGoalUsage {
        &self.usage
    }

    pub(crate) fn legacy_wire_representable(&self) -> bool {
        self.objective.legacy_wire_representable()
            && self.reason.len() <= 512
            && u16::try_from(self.criteria.len()).is_ok()
            && self.criteria.iter().all(|criterion| {
                criterion.definition.description.legacy_wire_representable()
            })
            && self.usage.legacy_wire_representable()
    }
}
