//! Checked local limits and monotonic counters.

use crate::{AgentErrorCode, AgentOperation, AgentRecovery, AgentRejection};

/// Local hard-limit dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AgentLimitDimension {
    ToolCalls,
    ProviderEvents,
    ContextCycles,
    OutputBytes,
    ToolResultBytes,
    ConcurrentToolCalls,
    Transitions,
}

/// Fully checked local D0 limits. These supplement, never replace, B1 budgets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the max prefix distinguishes immutable limits from counters"
)]
pub struct AgentLimits {
    max_tool_calls: u16,
    max_provider_events: u32,
    max_context_cycles: u16,
    max_output_bytes: u64,
    max_tool_result_bytes: u64,
    max_concurrent_tool_calls: u16,
    max_transitions: u32,
}

impl AgentLimits {
    pub const HARD_MAX_TOOL_CALLS: u16 = 256;
    pub const HARD_MAX_PROVIDER_EVENTS: u32 = 1_000_000;
    pub const HARD_MAX_CONTEXT_CYCLES: u16 = 4_096;
    pub const HARD_MAX_BYTES: u64 = 64 * 1024 * 1024;
    pub const HARD_MAX_CONCURRENT_TOOL_CALLS: u16 = 32;
    pub const HARD_MAX_TRANSITIONS: u32 = 1_000_000;

    /// Creates limits without clamping invalid input.
    ///
    /// # Errors
    ///
    /// Returns `InvalidLimit` when any value is zero or exceeds its production ceiling.
    #[allow(clippy::too_many_arguments, reason = "all local limit dimensions must be explicit")]
    pub const fn new(
        max_tool_calls: u16,
        max_provider_events: u32,
        max_context_cycles: u16,
        max_output_bytes: u64,
        max_tool_result_bytes: u64,
        max_concurrent_tool_calls: u16,
        max_transitions: u32,
    ) -> Result<Self, AgentRejection> {
        if max_tool_calls == 0
            || max_tool_calls > Self::HARD_MAX_TOOL_CALLS
            || max_provider_events == 0
            || max_provider_events > Self::HARD_MAX_PROVIDER_EVENTS
            || max_context_cycles == 0
            || max_context_cycles > Self::HARD_MAX_CONTEXT_CYCLES
            || max_output_bytes == 0
            || max_output_bytes > Self::HARD_MAX_BYTES
            || max_tool_result_bytes == 0
            || max_tool_result_bytes > Self::HARD_MAX_BYTES
            || max_concurrent_tool_calls == 0
            || max_concurrent_tool_calls > Self::HARD_MAX_CONCURRENT_TOOL_CALLS
            || max_transitions == 0
            || max_transitions > Self::HARD_MAX_TRANSITIONS
        {
            Err(AgentRejection::new(
                AgentErrorCode::InvalidLimit,
                AgentOperation::ValidateLimits,
                AgentRecovery::CorrectRequest,
                "agent limit is zero or exceeds the production hard ceiling",
            ))
        } else {
            Ok(Self {
                max_tool_calls,
                max_provider_events,
                max_context_cycles,
                max_output_bytes,
                max_tool_result_bytes,
                max_concurrent_tool_calls,
                max_transitions,
            })
        }
    }

    #[must_use]
    pub const fn max_tool_calls(self) -> u16 {
        self.max_tool_calls
    }
    #[must_use]
    pub const fn max_provider_events(self) -> u32 {
        self.max_provider_events
    }
    #[must_use]
    pub const fn max_context_cycles(self) -> u16 {
        self.max_context_cycles
    }
    #[must_use]
    pub const fn max_output_bytes(self) -> u64 {
        self.max_output_bytes
    }
    #[must_use]
    pub const fn max_tool_result_bytes(self) -> u64 {
        self.max_tool_result_bytes
    }
    #[must_use]
    pub const fn max_concurrent_tool_calls(self) -> u16 {
        self.max_concurrent_tool_calls
    }
    #[must_use]
    pub const fn max_transitions(self) -> u32 {
        self.max_transitions
    }
}

/// Monotonic resource-accounting projection retained in replayable state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AgentCounters {
    tool_calls: u16,
    provider_events: u32,
    context_cycles: u16,
    output_bytes: u64,
    tool_result_bytes: u64,
    active_tool_calls: u16,
    peak_concurrent_tool_calls: u16,
    transitions: u32,
}

impl AgentCounters {
    #[must_use]
    pub const fn tool_calls(self) -> u16 {
        self.tool_calls
    }
    #[must_use]
    pub const fn provider_events(self) -> u32 {
        self.provider_events
    }
    #[must_use]
    pub const fn context_cycles(self) -> u16 {
        self.context_cycles
    }
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }
    #[must_use]
    pub const fn tool_result_bytes(self) -> u64 {
        self.tool_result_bytes
    }
    #[must_use]
    pub const fn active_tool_calls(self) -> u16 {
        self.active_tool_calls
    }
    #[must_use]
    pub const fn peak_concurrent_tool_calls(self) -> u16 {
        self.peak_concurrent_tool_calls
    }
    #[must_use]
    pub const fn transitions(self) -> u32 {
        self.transitions
    }

    pub(crate) fn transition(&mut self, limits: AgentLimits) -> Result<(), AgentRejection> {
        self.transitions = checked_u32(self.transitions, 1, AgentLimitDimension::Transitions)?;
        check(self.transitions <= limits.max_transitions, AgentLimitDimension::Transitions)
    }

    pub(crate) fn context_cycle(&mut self, limits: AgentLimits) -> Result<(), AgentRejection> {
        self.context_cycles =
            checked_u16(self.context_cycles, 1, AgentLimitDimension::ContextCycles)?;
        check(self.context_cycles <= limits.max_context_cycles, AgentLimitDimension::ContextCycles)
    }

    pub(crate) fn provider_event(
        &mut self,
        bytes: u64,
        limits: AgentLimits,
    ) -> Result<(), AgentRejection> {
        self.provider_events =
            checked_u32(self.provider_events, 1, AgentLimitDimension::ProviderEvents)?;
        self.output_bytes =
            checked_u64(self.output_bytes, bytes, AgentLimitDimension::OutputBytes)?;
        check(
            self.provider_events <= limits.max_provider_events,
            AgentLimitDimension::ProviderEvents,
        )?;
        check(self.output_bytes <= limits.max_output_bytes, AgentLimitDimension::OutputBytes)
    }

    pub(crate) fn add_tools(
        &mut self,
        count: u16,
        limits: AgentLimits,
    ) -> Result<(), AgentRejection> {
        self.tool_calls = checked_u16(self.tool_calls, count, AgentLimitDimension::ToolCalls)?;
        check(self.tool_calls <= limits.max_tool_calls, AgentLimitDimension::ToolCalls)
    }

    pub(crate) fn start_tool(&mut self, limits: AgentLimits) -> Result<(), AgentRejection> {
        self.active_tool_calls =
            checked_u16(self.active_tool_calls, 1, AgentLimitDimension::ConcurrentToolCalls)?;
        self.peak_concurrent_tool_calls =
            self.peak_concurrent_tool_calls.max(self.active_tool_calls);
        check(
            self.active_tool_calls <= limits.max_concurrent_tool_calls,
            AgentLimitDimension::ConcurrentToolCalls,
        )
    }

    pub(crate) fn finish_tool(
        &mut self,
        bytes: u64,
        limits: AgentLimits,
    ) -> Result<(), AgentRejection> {
        self.active_tool_calls = self
            .active_tool_calls
            .checked_sub(1)
            .ok_or_else(|| invalid("active tool counter underflow"))?;
        self.tool_result_bytes =
            checked_u64(self.tool_result_bytes, bytes, AgentLimitDimension::ToolResultBytes)?;
        check(
            self.tool_result_bytes <= limits.max_tool_result_bytes,
            AgentLimitDimension::ToolResultBytes,
        )
    }
}

const fn check(valid: bool, _dimension: AgentLimitDimension) -> Result<(), AgentRejection> {
    if valid { Ok(()) } else { Err(limit_error()) }
}

fn checked_u16(
    value: u16,
    add: u16,
    _dimension: AgentLimitDimension,
) -> Result<u16, AgentRejection> {
    value.checked_add(add).ok_or_else(overflow)
}
fn checked_u32(
    value: u32,
    add: u32,
    _dimension: AgentLimitDimension,
) -> Result<u32, AgentRejection> {
    value.checked_add(add).ok_or_else(overflow)
}
fn checked_u64(
    value: u64,
    add: u64,
    _dimension: AgentLimitDimension,
) -> Result<u64, AgentRejection> {
    value.checked_add(add).ok_or_else(overflow)
}
const fn limit_error() -> AgentRejection {
    AgentRejection::new(
        AgentErrorCode::LimitExceeded,
        AgentOperation::Reduce,
        AgentRecovery::Exhausted,
        "local agent limit exceeded",
    )
}
const fn overflow() -> AgentRejection {
    AgentRejection::new(
        AgentErrorCode::ArithmeticOverflow,
        AgentOperation::Reduce,
        AgentRecovery::Exhausted,
        "agent counter overflow",
    )
}
const fn invalid(detail: &'static str) -> AgentRejection {
    AgentRejection::new(
        AgentErrorCode::InvalidCommand,
        AgentOperation::Reduce,
        AgentRecovery::CorrectRequest,
        detail,
    )
}
