//! Goal usage, admission, and settlement value types.

use super::ControlError;
use serde::de::Error as _;
use serde::ser::SerializeStruct as _;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

const COUNTER_SCHEMA: u16 = 2;

/// An exact cumulative goal counter represented by canonical little-endian `u64` limbs.
///
/// Values inside the historical width retain their original scalar JSON representation. Larger
/// values use an explicit segmented representation, so existing operation and receipt identities
/// remain unchanged until the old representation is actually exhausted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalCounter<const LEGACY_BITS: u8> {
    segments: Vec<u64>,
}

/// Exact request/tool/retry count with a legacy `u32` scalar frontier.
pub type GoalCount = GoalCounter<32>;
/// Exact token/time/cost amount with a legacy `u64` scalar frontier.
pub type GoalAmount = GoalCounter<64>;

impl<const LEGACY_BITS: u8> GoalCounter<LEGACY_BITS> {
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
    /// Rejects unsupported legacy widths and limbs with a redundant most-significant zero.
    pub fn from_segments(segments: Vec<u64>) -> Result<Self, ControlError> {
        if Self::legacy_max().is_none() || segments.last() == Some(&0) {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { segments })
    }

    /// Borrows the canonical little-endian limbs; an empty slice represents zero.
    #[must_use]
    pub fn segments(&self) -> &[u64] {
        &self.segments
    }

    /// Returns the historical scalar representation when this value still fits it.
    #[must_use]
    pub fn legacy_value(&self) -> Option<u64> {
        let value = match self.segments.as_slice() {
            [] => 0,
            [value] => *value,
            _ => return None,
        };
        (value <= Self::legacy_max()?).then_some(value)
    }

    pub(super) fn increment(&mut self) {
        self.add_u64(1);
    }

    pub(super) fn add_u64(&mut self, value: u64) {
        if value == 0 {
            return;
        }
        let mut carry = value;
        let mut index = 0;
        while carry != 0 {
            if index == self.segments.len() {
                self.segments.push(carry);
                return;
            }
            let (sum, overflow) = self.segments[index].overflowing_add(carry);
            self.segments[index] = sum;
            carry = u64::from(overflow);
            index += 1;
        }
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

impl<const LEGACY_BITS: u8> Default for GoalCounter<LEGACY_BITS> {
    fn default() -> Self {
        Self { segments: Vec::new() }
    }
}

impl<const LEGACY_BITS: u8> Ord for GoalCounter<LEGACY_BITS> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.segments
            .len()
            .cmp(&other.segments.len())
            .then_with(|| self.segments.iter().rev().cmp(other.segments.iter().rev()))
    }
}

impl<const LEGACY_BITS: u8> PartialOrd for GoalCounter<LEGACY_BITS> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const LEGACY_BITS: u8> PartialEq<u64> for GoalCounter<LEGACY_BITS> {
    fn eq(&self, other: &u64) -> bool {
        match self.segments.as_slice() {
            [] => *other == 0,
            [value] => value == other,
            _ => false,
        }
    }
}

impl<const LEGACY_BITS: u8> PartialEq<GoalCounter<LEGACY_BITS>> for u64 {
    fn eq(&self, other: &GoalCounter<LEGACY_BITS>) -> bool {
        other == self
    }
}

impl<const LEGACY_BITS: u8> Serialize for GoalCounter<LEGACY_BITS> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(value) = self.legacy_value() {
            return serializer.serialize_u64(value);
        }
        let mut state = serializer.serialize_struct("GoalCounter", 2)?;
        state.serialize_field("schema", &COUNTER_SCHEMA)?;
        state.serialize_field("segments", &self.segments)?;
        state.end()
    }
}

impl<'de, const LEGACY_BITS: u8> Deserialize<'de> for GoalCounter<LEGACY_BITS> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Extended {
            schema: u16,
            segments: Vec<u64>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Representation {
            Legacy(u64),
            Extended(Extended),
        }

        match Representation::deserialize(deserializer)? {
            Representation::Legacy(value) => {
                let maximum = Self::legacy_max()
                    .ok_or_else(|| D::Error::custom("unsupported goal counter width"))?;
                if value > maximum {
                    return Err(D::Error::custom("noncanonical goal counter scalar"));
                }
                Ok(Self::from_u64(value))
            }
            Representation::Extended(value) => {
                let counter = Self::from_segments(value.segments).map_err(D::Error::custom)?;
                if value.schema != COUNTER_SCHEMA || counter.legacy_value().is_some() {
                    return Err(D::Error::custom("noncanonical segmented goal counter"));
                }
                Ok(counter)
            }
        }
    }
}

/// Provider usage attached to one completed reserved request. `None` remains unavailable.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
    pub(super) requests: GoalCount,
    pub(super) completed_requests: GoalCount,
    pub(super) token_reported_requests: GoalCount,
    pub(super) tool_calls: GoalCount,
    pub(super) input_tokens: GoalAmount,
    pub(super) cached_input_tokens: GoalAmount,
    pub(super) output_tokens: GoalAmount,
    pub(super) total_tokens: GoalAmount,
    pub(super) provider_cost_microunits: GoalAmount,
    pub(super) cost_reported_requests: GoalCount,
}

impl GoalRoleUsage {
    /// Reserved provider requests, including ambiguous or cancelled outcomes.
    #[must_use]
    pub fn requests(&self) -> GoalCount {
        self.requests.clone()
    }
    /// Requests with a conclusively observed terminal provider boundary.
    #[must_use]
    pub fn completed_requests(&self) -> GoalCount {
        self.completed_requests.clone()
    }
    /// Requests that supplied usable token counters.
    #[must_use]
    pub fn token_reported_requests(&self) -> GoalCount {
        self.token_reported_requests.clone()
    }
    /// Reserved tool operations.
    #[must_use]
    pub fn tool_calls(&self) -> GoalCount {
        self.tool_calls.clone()
    }
    /// Provider-reported input tokens; meaningful only when aggregate token usage is known.
    #[must_use]
    pub fn input_tokens(&self) -> GoalAmount {
        self.input_tokens.clone()
    }
    /// Provider-reported cached input tokens.
    #[must_use]
    pub fn cached_input_tokens(&self) -> GoalAmount {
        self.cached_input_tokens.clone()
    }
    /// Provider-reported output tokens.
    #[must_use]
    pub fn output_tokens(&self) -> GoalAmount {
        self.output_tokens.clone()
    }
    /// Explicit or conservatively derived aggregate tokens.
    #[must_use]
    pub fn total_tokens(&self) -> GoalAmount {
        self.total_tokens.clone()
    }
    /// Provider-estimated microunits, without currency semantics.
    #[must_use]
    pub fn provider_cost_microunits(&self) -> GoalAmount {
        self.provider_cost_microunits.clone()
    }
    /// Requests for which a cost counter was actually supplied, including a reported zero.
    #[must_use]
    pub fn cost_reported_requests(&self) -> GoalCount {
        self.cost_reported_requests.clone()
    }
    /// Whether every reserved request has terminal token reporting.
    #[must_use]
    pub fn tokens_known(&self) -> bool {
        self.requests == self.completed_requests && self.requests == self.token_reported_requests
    }
    /// Whether every reserved request has a provider cost observation.
    #[must_use]
    pub fn cost_known(&self) -> bool {
        self.requests == self.completed_requests && self.requests == self.cost_reported_requests
    }
}

/// Goal-wide cumulative ledger. Counts never reset on attempt retry or daemon restart.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalUsage {
    pub(super) roles: [GoalRoleUsage; 3],
    pub(super) active_millis: GoalAmount,
    pub(super) retries: GoalCount,
    pub(super) provider_failovers: GoalCount,
    pub(super) compactions: GoalCount,
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
    pub fn requests(&self) -> GoalCount {
        self.roles.iter().fold(GoalCount::default(), |mut sum, role| {
            sum.add_counter(&role.requests);
            sum
        })
    }
    /// Sum of reserved tool calls across every role and attempt.
    #[must_use]
    pub fn tool_calls(&self) -> GoalCount {
        self.roles.iter().fold(GoalCount::default(), |mut sum, role| {
            sum.add_counter(&role.tool_calls);
            sum
        })
    }
    /// Active execution time observed at completed runner boundaries; paused time is excluded.
    #[must_use]
    pub fn active_millis(&self) -> GoalAmount {
        self.active_millis.clone()
    }
    /// Checked retry count accumulated across attempts.
    #[must_use]
    pub fn retries(&self) -> GoalCount {
        self.retries.clone()
    }
    /// Explicit configured provider failovers across attempts.
    #[must_use]
    pub fn provider_failovers(&self) -> GoalCount {
        self.provider_failovers.clone()
    }
    /// Deterministic context compactions across attempts.
    #[must_use]
    pub fn compactions(&self) -> GoalCount {
        self.compactions.clone()
    }
    /// Cumulative total tokens. Consult [`Self::tokens_known`] before presenting zero as known.
    #[must_use]
    pub fn total_tokens(&self) -> GoalAmount {
        self.roles.iter().fold(GoalAmount::default(), |mut sum, role| {
            sum.add_counter(&role.total_tokens);
            sum
        })
    }
    /// Provider-estimated microunits. No currency is defined by this counter.
    #[must_use]
    pub fn provider_cost_microunits(&self) -> GoalAmount {
        self.roles.iter().fold(GoalAmount::default(), |mut sum, role| {
            sum.add_counter(&role.provider_cost_microunits);
            sum
        })
    }
    /// True only when every reserved request completed with token usage.
    #[must_use]
    pub fn tokens_known(&self) -> bool {
        self.roles.iter().all(|role| role.tokens_known())
    }
    /// True only when every reserved request completed with an explicit cost observation.
    #[must_use]
    pub fn cost_known(&self) -> bool {
        self.roles.iter().all(|role| role.cost_known())
    }
    /// Latest workspace byte observation.
    #[must_use]
    pub const fn workspace_bytes(&self) -> u64 {
        self.workspace_bytes
    }
    /// Maximum positive workspace growth observed over any attempt.
    #[must_use]
    pub const fn workspace_growth_bytes(&self) -> u64 {
        self.workspace_growth_bytes
    }
    /// Maximum resident set observation over any attempt.
    #[must_use]
    pub const fn peak_rss_bytes(&self) -> u64 {
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
