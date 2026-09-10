//! Goal budget, usage, admission, and settlement value types.

use serde::Deserialize;
use serde::Serialize;

use crate::{
    PRODUCT_RUN_MAX_ELAPSED, PRODUCT_RUN_MAX_MODEL_REQUESTS, PRODUCT_RUN_MAX_TOOL_CALLS,
    PRODUCT_RUN_MAX_TOTAL_TOKENS, control::ControlError,
};

/// Optional user limits. Effective limits are capped by existing host hard ceilings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalBudget {
    #[serde(rename = "max_active_millis")]
    pub(super) active_millis: Option<u64>,
    #[serde(rename = "max_requests")]
    pub(super) requests: Option<u32>,
    #[serde(rename = "max_tool_calls")]
    pub(super) tool_calls: Option<u32>,
    #[serde(rename = "max_total_tokens")]
    pub(super) total_tokens: Option<u64>,
}

impl GoalBudget {
    /// Constructs checked user limits without claiming a guaranteed monetary cap.
    ///
    /// # Errors
    /// Rejects zero values or limits wider than the installed host ceilings.
    pub fn new(
        max_active_millis: Option<u64>,
        max_requests: Option<u32>,
        max_tool_calls: Option<u32>,
        max_total_tokens: Option<u64>,
    ) -> Result<Self, ControlError> {
        let hard_millis = u64::try_from(PRODUCT_RUN_MAX_ELAPSED.as_millis()).unwrap_or(u64::MAX);
        if max_active_millis.is_some_and(|value| value == 0 || value > hard_millis)
            || max_requests
                .is_some_and(|value| value == 0 || value > PRODUCT_RUN_MAX_MODEL_REQUESTS)
            || max_tool_calls.is_some_and(|value| value == 0 || value > PRODUCT_RUN_MAX_TOOL_CALLS)
            || max_total_tokens
                .is_some_and(|value| value == 0 || value > PRODUCT_RUN_MAX_TOTAL_TOKENS)
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self {
            active_millis: max_active_millis,
            requests: max_requests,
            tool_calls: max_tool_calls,
            total_tokens: max_total_tokens,
        })
    }

    /// User-selected active execution time limit.
    #[must_use]
    pub const fn max_active_millis(self) -> Option<u64> {
        self.active_millis
    }
    /// User-selected admitted provider-request limit.
    #[must_use]
    pub const fn max_requests(self) -> Option<u32> {
        self.requests
    }
    /// User-selected admitted tool-call limit.
    #[must_use]
    pub const fn max_tool_calls(self) -> Option<u32> {
        self.tool_calls
    }
    /// Best-effort stop threshold over normalized reported/derived token usage.
    #[must_use]
    pub const fn max_total_tokens(self) -> Option<u64> {
        self.total_tokens
    }

    pub(super) const fn effective_active_millis(self) -> u64 {
        match self.active_millis {
            Some(value) => value,
            None => u64::MAX,
        }
    }
    pub(super) const fn effective_requests(self) -> u32 {
        match self.requests {
            Some(value) => value,
            None => PRODUCT_RUN_MAX_MODEL_REQUESTS,
        }
    }
    pub(super) const fn effective_tools(self) -> u32 {
        match self.tool_calls {
            Some(value) => value,
            None => PRODUCT_RUN_MAX_TOOL_CALLS,
        }
    }
    pub(super) const fn effective_tokens(self) -> u64 {
        match self.total_tokens {
            Some(value) => value,
            None => PRODUCT_RUN_MAX_TOTAL_TOKENS,
        }
    }
}

/// Provider usage attached to one completed reserved request. `None` remains unavailable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalUsageReport {
    /// Provider-reported input tokens.
    pub input_tokens: Option<u64>,
    /// Provider-reported cache-read input tokens.
    pub cached_input_tokens: Option<u64>,
    /// Provider-reported output tokens.
    pub output_tokens: Option<u64>,
    /// Explicit total, or a locally derived input-plus-output total.
    pub total_tokens: Option<u64>,
    /// Provider-estimated integer microunits; no currency is implied.
    pub provider_cost_microunits: Option<u64>,
}

impl GoalUsageReport {
    pub(super) const fn has_tokens(self) -> bool {
        self.total_tokens.is_some() || self.input_tokens.is_some() || self.output_tokens.is_some()
    }
}

/// Exact locally attributed cumulative consumption for one role.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalRoleUsage {
    pub(super) requests: u32,
    pub(super) completed_requests: u32,
    pub(super) token_reported_requests: u32,
    pub(super) tool_calls: u32,
    pub(super) input_tokens: u64,
    pub(super) cached_input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) provider_cost_microunits: u64,
    pub(super) cost_reported_requests: u32,
}

impl GoalRoleUsage {
    /// Reserved provider requests, including ambiguous or cancelled outcomes.
    #[must_use]
    pub const fn requests(self) -> u32 {
        self.requests
    }
    /// Requests with a conclusively observed terminal provider boundary.
    #[must_use]
    pub const fn completed_requests(self) -> u32 {
        self.completed_requests
    }
    /// Requests that supplied usable token counters.
    #[must_use]
    pub const fn token_reported_requests(self) -> u32 {
        self.token_reported_requests
    }
    /// Reserved tool operations.
    #[must_use]
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Provider-reported input tokens; meaningful only when aggregate token usage is known.
    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    /// Provider-reported cached input tokens.
    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    /// Provider-reported output tokens.
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    /// Explicit or conservatively derived aggregate tokens.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    /// Provider-estimated microunits, without currency semantics.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }
    /// Requests for which a cost counter was actually supplied, including a reported zero.
    #[must_use]
    pub const fn cost_reported_requests(self) -> u32 {
        self.cost_reported_requests
    }
    /// Whether every reserved request has terminal token reporting.
    #[must_use]
    pub const fn tokens_known(self) -> bool {
        self.requests == self.completed_requests && self.requests == self.token_reported_requests
    }
    /// Whether every reserved request has a provider cost observation.
    #[must_use]
    pub const fn cost_known(self) -> bool {
        self.requests == self.completed_requests && self.requests == self.cost_reported_requests
    }
}

/// Goal-wide cumulative ledger. Counts never reset on attempt retry or daemon restart.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalUsage {
    pub(super) roles: [GoalRoleUsage; 3],
    pub(super) active_millis: u64,
    pub(super) retries: u32,
    pub(super) provider_failovers: u32,
    pub(super) compactions: u32,
    pub(super) workspace_bytes: u64,
    pub(super) workspace_growth_bytes: u64,
    pub(super) peak_rss_bytes: u64,
}

impl GoalUsage {
    /// Exact locally attributed rows in writer, reviewer, fixer order.
    #[must_use]
    pub const fn roles(&self) -> &[GoalRoleUsage; 3] {
        &self.roles
    }
    /// Sum of reserved requests across every role and attempt.
    #[must_use]
    pub fn requests(self) -> u32 {
        self.roles.iter().fold(0, |sum, role| sum.saturating_add(role.requests))
    }
    /// Sum of reserved tool calls across every role and attempt.
    #[must_use]
    pub fn tool_calls(self) -> u32 {
        self.roles.iter().fold(0, |sum, role| sum.saturating_add(role.tool_calls))
    }
    /// Active execution time observed at completed runner boundaries; paused time is excluded.
    #[must_use]
    pub const fn active_millis(self) -> u64 {
        self.active_millis
    }
    /// Checked retry count accumulated across attempts.
    #[must_use]
    pub const fn retries(self) -> u32 {
        self.retries
    }
    /// Explicit configured provider failovers across attempts.
    #[must_use]
    pub const fn provider_failovers(self) -> u32 {
        self.provider_failovers
    }
    /// Deterministic context compactions across attempts.
    #[must_use]
    pub const fn compactions(self) -> u32 {
        self.compactions
    }
    /// Cumulative total tokens. Consult [`Self::tokens_known`] before presenting zero as known.
    #[must_use]
    pub fn total_tokens(self) -> u64 {
        self.roles.iter().fold(0, |sum, role| sum.saturating_add(role.total_tokens))
    }
    /// Provider-estimated microunits. No currency is defined by this counter.
    #[must_use]
    pub fn provider_cost_microunits(self) -> u64 {
        self.roles.iter().fold(0, |sum, role| sum.saturating_add(role.provider_cost_microunits))
    }
    /// True only when every reserved request completed with token usage.
    #[must_use]
    pub fn tokens_known(self) -> bool {
        self.roles.iter().all(|role| role.tokens_known())
    }
    /// True only when every reserved request completed with an explicit cost observation.
    #[must_use]
    pub fn cost_known(self) -> bool {
        self.roles.iter().all(|role| role.cost_known())
    }
    /// Latest workspace byte observation.
    #[must_use]
    pub const fn workspace_bytes(self) -> u64 {
        self.workspace_bytes
    }
    /// Maximum positive workspace growth observed over any attempt.
    #[must_use]
    pub const fn workspace_growth_bytes(self) -> u64 {
        self.workspace_growth_bytes
    }
    /// Maximum resident set observation over any attempt.
    #[must_use]
    pub const fn peak_rss_bytes(self) -> u64 {
        self.peak_rss_bytes
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GoalAttemptProgress {
    pub(super) elapsed_millis: u64,
    pub(super) retries: u32,
    pub(super) provider_failovers: u32,
    pub(super) compactions: u32,
}

/// Result of one durable safe-boundary operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalAdmission {
    /// The operation was durably reserved and may start.
    Accepted,
    /// A durable pause/cancel state prevents the operation.
    Paused,
    /// A cumulative hard or user limit prevents the operation.
    BudgetReached,
    /// The goal is waiting, blocked, achieved, cancelled, or otherwise not runnable.
    Inactive,
}

/// Host-observed runner settlement facts. No provider-authored completion claim is accepted here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalSettlement {
    /// Strict runner settlement was accepted with current evidence and no outstanding work.
    Accepted,
    /// The runner requires material user input.
    WaitingForUser,
    /// Recovery is required before any mutation can continue.
    RecoveryRequired,
    /// Work ended without satisfying the current goal.
    Failed,
    /// Owned execution was cancelled or stopped.
    Cancelled,
}
