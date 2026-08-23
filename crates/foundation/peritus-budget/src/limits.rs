//! Budget dimensions, exact dimension sets, and immutable account ceilings.

use crate::BudgetAmounts;
use vstd::prelude::*;

verus! {

/// One monotonic budget dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BudgetDimension {
    /// Provider input and output tokens.
    ModelTokens,
    /// Provider cost in millionths of the configured currency unit.
    ProviderCostMicrounits,
    /// Accountable active-effect duration in milliseconds.
    ActiveEffectMilliseconds,
    /// Total execution attempts, including the first attempt.
    Attempts,
    /// Attempts after the first attempt.
    Retries,
}

/// A compact set of budget dimensions.
#[allow(
    clippy::struct_excessive_bools,
    reason = "The five closed budget dimensions intentionally map one-to-one to stable set bits"
)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetDimensionSet {
    model_tokens: bool,
    provider_cost: bool,
    active_effect: bool,
    attempts: bool,
    retries: bool,
}

impl BudgetDimensionSet {
    /// Returns the empty set.
    #[must_use]
    pub const fn empty() -> (result: Self)
        ensures result.spec_is_empty(),
    {
        Self {
            model_tokens: false,
            provider_cost: false,
            active_effect: false,
            attempts: false,
            retries: false,
        }
    }

    /// Mathematical emptiness view used by refinement contracts.
    pub closed spec fn spec_is_empty(&self) -> bool {
        !self.model_tokens
            && !self.provider_cost
            && !self.active_effect
            && !self.attempts
            && !self.retries
    }

    /// Mathematical bit representation used by exact failure contracts.
    pub closed spec fn spec_bits(&self) -> int {
        (if self.model_tokens { 1int } else { 0int })
            + (if self.provider_cost { 2int } else { 0int })
            + (if self.active_effect { 4int } else { 0int })
            + (if self.attempts { 8int } else { 0int })
            + (if self.retries { 16int } else { 0int })
    }

    /// Returns whether no dimensions are present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.model_tokens
            && !self.provider_cost
            && !self.active_effect
            && !self.attempts
            && !self.retries
    }

    /// Returns whether `dimension` is present.
    #[must_use]
    pub const fn contains(self, dimension: BudgetDimension) -> bool {
        match dimension {
            BudgetDimension::ModelTokens => self.model_tokens,
            BudgetDimension::ProviderCostMicrounits => self.provider_cost,
            BudgetDimension::ActiveEffectMilliseconds => self.active_effect,
            BudgetDimension::Attempts => self.attempts,
            BudgetDimension::Retries => self.retries,
        }
    }

    #[allow(
        clippy::redundant_pub_crate,
        clippy::fn_params_excessive_bools,
        reason = "Verus requires crate visibility for this cross-module exact bitset constructor"
    )]
    pub(crate) const fn from_members(
        model_tokens: bool,
        provider_cost: bool,
        active_effect: bool,
        attempts: bool,
        retries: bool,
    ) -> (result: Self)
        ensures
            result.spec_bits() == (if model_tokens { 1int } else { 0int })
                + (if provider_cost { 2int } else { 0int })
                + (if active_effect { 4int } else { 0int })
                + (if attempts { 8int } else { 0int })
                + (if retries { 16int } else { 0int }),
    {
        Self { model_tokens, provider_cost, active_effect, attempts, retries }
    }
}

/// Immutable finite ceilings for one budget account.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetLimits {
    amounts: BudgetAmounts,
}

impl BudgetLimits {
    /// Creates immutable finite limits. Zero is a valid immediately exhausted dimension.
    #[must_use]
    pub const fn new(amounts: BudgetAmounts) -> Self {
        Self { amounts }
    }

    /// Returns the complete limit vector.
    #[must_use]
    pub const fn amounts(self) -> (amounts: BudgetAmounts)
        ensures amounts == self.spec_amounts(),
    {
        self.amounts
    }

    /// Returns the mathematical amount vector.
    pub closed spec fn spec_amounts(&self) -> BudgetAmounts { self.amounts }
}

} // verus!
