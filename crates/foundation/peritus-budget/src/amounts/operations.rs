//! Componentwise comparisons and checked arithmetic.

use super::{BudgetAmounts, add, sub};
use crate::{AmountArithmeticError, BudgetDimension, BudgetDimensionSet};
use vstd::prelude::*;

verus! {

impl BudgetAmounts {
    /// Returns whether every dimension is zero.
    #[must_use]
    pub const fn is_zero(self) -> (result: bool)
        ensures result == self.spec_is_zero(),
    {
        self.model_tokens.get() == 0
            && self.provider_cost_microunits.get() == 0
            && self.active_effect_milliseconds.get() == 0
            && self.attempts.get() == 0
            && self.retries.get() == 0
    }

    /// Returns whether this amount is componentwise no greater than `ceiling`.
    #[must_use]
    pub const fn fits_within(self, ceiling: Self) -> (result: bool)
        ensures result == self.spec_le(ceiling),
    {
        self.model_tokens.get() <= ceiling.model_tokens.get()
            && self.provider_cost_microunits.get() <= ceiling.provider_cost_microunits.get()
            && self.active_effect_milliseconds.get()
                <= ceiling.active_effect_milliseconds.get()
            && self.attempts.get() <= ceiling.attempts.get()
            && self.retries.get() <= ceiling.retries.get()
    }

    pub(crate) const fn equals(self, other: Self) -> (result: bool)
        ensures result == self.spec_equal(other),
    {
        self.model_tokens.get() == other.model_tokens.get()
            && self.provider_cost_microunits.get() == other.provider_cost_microunits.get()
            && self.active_effect_milliseconds.get()
                == other.active_effect_milliseconds.get()
            && self.attempts.get() == other.attempts.get()
            && self.retries.get() == other.retries.get()
    }

    /// Returns dimensions in which this amount exceeds `ceiling`.
    #[must_use]
    pub const fn exceeding_dimensions(
        self,
        ceiling: Self,
    ) -> (result: BudgetDimensionSet)
        ensures result.spec_bits() == Self::spec_exceeding_bits(self, ceiling),
    {
        BudgetDimensionSet::from_members(
            self.model_tokens.get() > ceiling.model_tokens.get(),
            self.provider_cost_microunits.get() > ceiling.provider_cost_microunits.get(),
            self.active_effect_milliseconds.get() > ceiling.active_effect_milliseconds.get(),
            self.attempts.get() > ceiling.attempts.get(),
            self.retries.get() > ceiling.retries.get(),
        )
    }

    /// Exact stable bitset of dimensions where `amount` exceeds `ceiling`.
    pub open spec fn spec_exceeding_bits(amount: Self, ceiling: Self) -> int {
        (if amount.spec_get(BudgetDimension::ModelTokens)
                > ceiling.spec_get(BudgetDimension::ModelTokens) { 1int } else { 0int })
            + (if amount.spec_get(BudgetDimension::ProviderCostMicrounits)
                > ceiling.spec_get(BudgetDimension::ProviderCostMicrounits) { 2int } else { 0int })
            + (if amount.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                > ceiling.spec_get(BudgetDimension::ActiveEffectMilliseconds) { 4int } else { 0int })
            + (if amount.spec_get(BudgetDimension::Attempts)
                > ceiling.spec_get(BudgetDimension::Attempts) { 8int } else { 0int })
            + (if amount.spec_get(BudgetDimension::Retries)
                > ceiling.spec_get(BudgetDimension::Retries) { 16int } else { 0int })
    }

    /// Adds every dimension exactly.
    ///
    /// # Errors
    ///
    /// Returns the first dimension whose sum exceeds `u64`.
    pub const fn checked_add(
        self,
        rhs: Self,
    ) -> (result: Result<Self, AmountArithmeticError>)
        ensures
            match result {
                Ok(sum) => {
                    Self::spec_sum(sum, self, rhs)
                        && !Self::spec_addition_overflows(self, rhs)
                }
                Err(error) => Self::addition_error_exact(error, self, rhs),
            },
    {
        let model_tokens = add(
            self.model_tokens,
            rhs.model_tokens,
            BudgetDimension::ModelTokens,
        );
        let model_tokens = match model_tokens { Ok(value) => value, Err(error) => return Err(error) };
        assert(self.spec_get(BudgetDimension::ModelTokens)
            + rhs.spec_get(BudgetDimension::ModelTokens) <= u64::MAX);
        let provider_cost_microunits = add(
            self.provider_cost_microunits,
            rhs.provider_cost_microunits,
            BudgetDimension::ProviderCostMicrounits,
        );
        let provider_cost_microunits = match provider_cost_microunits { Ok(value) => value, Err(error) => return Err(error) };
        assert(self.spec_get(BudgetDimension::ProviderCostMicrounits)
            + rhs.spec_get(BudgetDimension::ProviderCostMicrounits) <= u64::MAX);
        let active_effect_milliseconds = add(
            self.active_effect_milliseconds,
            rhs.active_effect_milliseconds,
            BudgetDimension::ActiveEffectMilliseconds,
        );
        let active_effect_milliseconds = match active_effect_milliseconds { Ok(value) => value, Err(error) => return Err(error) };
        assert(self.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            + rhs.spec_get(BudgetDimension::ActiveEffectMilliseconds) <= u64::MAX);
        let attempts = add(self.attempts, rhs.attempts, BudgetDimension::Attempts);
        let attempts = match attempts { Ok(value) => value, Err(error) => return Err(error) };
        assert(self.spec_get(BudgetDimension::Attempts)
            + rhs.spec_get(BudgetDimension::Attempts) <= u64::MAX);
        let retries = add(self.retries, rhs.retries, BudgetDimension::Retries);
        let retries = match retries { Ok(value) => value, Err(error) => return Err(error) };
        assert(self.spec_get(BudgetDimension::Retries)
            + rhs.spec_get(BudgetDimension::Retries) <= u64::MAX);
        Ok(Self::new(
            model_tokens,
            provider_cost_microunits,
            active_effect_milliseconds,
            attempts,
            retries,
        ))
    }

    /// Subtracts every dimension exactly.
    ///
    /// # Errors
    ///
    /// Returns the first dimension whose difference would be negative.
    pub const fn checked_sub(
        self,
        rhs: Self,
    ) -> (result: Result<Self, AmountArithmeticError>)
        ensures
            match result {
                Ok(difference) => Self::spec_difference(difference, self, rhs),
                Err(error) => Self::subtraction_error_exact(error, self, rhs),
            },
    {
        let model_tokens = sub(
            self.model_tokens,
            rhs.model_tokens,
            BudgetDimension::ModelTokens,
        );
        let model_tokens = match model_tokens { Ok(value) => value, Err(error) => return Err(error) };
        assert(rhs.spec_get(BudgetDimension::ModelTokens)
            <= self.spec_get(BudgetDimension::ModelTokens));
        let provider_cost_microunits = sub(
            self.provider_cost_microunits,
            rhs.provider_cost_microunits,
            BudgetDimension::ProviderCostMicrounits,
        );
        let provider_cost_microunits = match provider_cost_microunits { Ok(value) => value, Err(error) => return Err(error) };
        assert(rhs.spec_get(BudgetDimension::ProviderCostMicrounits)
            <= self.spec_get(BudgetDimension::ProviderCostMicrounits));
        let active_effect_milliseconds = sub(
            self.active_effect_milliseconds,
            rhs.active_effect_milliseconds,
            BudgetDimension::ActiveEffectMilliseconds,
        );
        let active_effect_milliseconds = match active_effect_milliseconds { Ok(value) => value, Err(error) => return Err(error) };
        assert(rhs.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            <= self.spec_get(BudgetDimension::ActiveEffectMilliseconds));
        let attempts = sub(self.attempts, rhs.attempts, BudgetDimension::Attempts);
        let attempts = match attempts { Ok(value) => value, Err(error) => return Err(error) };
        assert(rhs.spec_get(BudgetDimension::Attempts)
            <= self.spec_get(BudgetDimension::Attempts));
        let retries = sub(self.retries, rhs.retries, BudgetDimension::Retries);
        let retries = match retries { Ok(value) => value, Err(error) => return Err(error) };
        Ok(Self::new(
            model_tokens,
            provider_cost_microunits,
            active_effect_milliseconds,
            attempts,
            retries,
        ))
    }
}

} // verus!
