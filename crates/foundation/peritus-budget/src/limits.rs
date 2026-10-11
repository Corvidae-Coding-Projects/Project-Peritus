//! Budget dimensions, exact dimension sets, and immutable account ceilings.

use crate::BudgetAmounts;

#[doc = " One monotonic budget dimension."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BudgetDimension {
    #[doc = " Provider input and output tokens."]
    ModelTokens,
    #[doc = " Provider cost in millionths of the configured currency unit."]
    ProviderCostMicrounits,
    #[doc = " Accountable active-effect duration in milliseconds."]
    ActiveEffectMilliseconds,
    #[doc = " Total execution attempts, including the first attempt."]
    Attempts,
    #[doc = " Attempts after the first attempt."]
    Retries,
}
#[doc = " A compact set of budget dimensions."]
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
    #[doc = " Returns the empty set."]
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            model_tokens: false,
            provider_cost: false,
            active_effect: false,
            attempts: false,
            retries: false,
        }
    }
    #[doc = " Returns whether no dimensions are present."]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.model_tokens
            && !self.provider_cost
            && !self.active_effect
            && !self.attempts
            && !self.retries
    }
    #[doc = " Returns whether `dimension` is present."]
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
    ) -> Self {
        Self { model_tokens, provider_cost, active_effect, attempts, retries }
    }
}
#[doc = " Immutable finite ceilings for one budget account."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetLimits {
    amounts: BudgetAmounts,
}
impl BudgetLimits {
    #[doc = " Creates immutable finite limits. Zero is a valid immediately exhausted dimension."]
    #[must_use]
    pub const fn new(amounts: BudgetAmounts) -> Self {
        Self { amounts }
    }
    #[doc = " Returns the complete limit vector."]
    #[must_use]
    pub const fn amounts(self) -> BudgetAmounts {
        self.amounts
    }
}
