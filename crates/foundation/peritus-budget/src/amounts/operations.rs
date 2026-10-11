//! Componentwise comparisons and checked arithmetic.

use super::{BudgetAmounts, add, sub};
use crate::{AmountArithmeticError, BudgetDimension, BudgetDimensionSet};

impl BudgetAmounts {
    #[doc = " Returns whether every dimension is zero."]
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.model_tokens.get() == 0
            && self.provider_cost_microunits.get() == 0
            && self.active_effect_milliseconds.get() == 0
            && self.attempts.get() == 0
            && self.retries.get() == 0
    }
    #[doc = " Returns whether this amount is componentwise no greater than `ceiling`."]
    #[must_use]
    pub const fn fits_within(self, ceiling: Self) -> bool {
        self.model_tokens.get() <= ceiling.model_tokens.get()
            && self.provider_cost_microunits.get() <= ceiling.provider_cost_microunits.get()
            && self.active_effect_milliseconds.get() <= ceiling.active_effect_milliseconds.get()
            && self.attempts.get() <= ceiling.attempts.get()
            && self.retries.get() <= ceiling.retries.get()
    }
    pub(crate) const fn equals(self, other: Self) -> bool {
        self.model_tokens.get() == other.model_tokens.get()
            && self.provider_cost_microunits.get() == other.provider_cost_microunits.get()
            && self.active_effect_milliseconds.get() == other.active_effect_milliseconds.get()
            && self.attempts.get() == other.attempts.get()
            && self.retries.get() == other.retries.get()
    }
    #[doc = " Returns dimensions in which this amount exceeds `ceiling`."]
    #[must_use]
    pub const fn exceeding_dimensions(self, ceiling: Self) -> BudgetDimensionSet {
        BudgetDimensionSet::from_members(
            self.model_tokens.get() > ceiling.model_tokens.get(),
            self.provider_cost_microunits.get() > ceiling.provider_cost_microunits.get(),
            self.active_effect_milliseconds.get() > ceiling.active_effect_milliseconds.get(),
            self.attempts.get() > ceiling.attempts.get(),
            self.retries.get() > ceiling.retries.get(),
        )
    }
    #[doc = " Adds every dimension exactly."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the first dimension whose sum exceeds `u64`."]
    pub const fn checked_add(self, rhs: Self) -> Result<Self, AmountArithmeticError> {
        let model_tokens = add(self.model_tokens, rhs.model_tokens, BudgetDimension::ModelTokens);
        let model_tokens = match model_tokens {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let provider_cost_microunits = add(
            self.provider_cost_microunits,
            rhs.provider_cost_microunits,
            BudgetDimension::ProviderCostMicrounits,
        );
        let provider_cost_microunits = match provider_cost_microunits {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let active_effect_milliseconds = add(
            self.active_effect_milliseconds,
            rhs.active_effect_milliseconds,
            BudgetDimension::ActiveEffectMilliseconds,
        );
        let active_effect_milliseconds = match active_effect_milliseconds {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let attempts = add(self.attempts, rhs.attempts, BudgetDimension::Attempts);
        let attempts = match attempts {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let retries = add(self.retries, rhs.retries, BudgetDimension::Retries);
        let retries = match retries {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        Ok(Self::new(
            model_tokens,
            provider_cost_microunits,
            active_effect_milliseconds,
            attempts,
            retries,
        ))
    }
    #[doc = " Subtracts every dimension exactly."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the first dimension whose difference would be negative."]
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, AmountArithmeticError> {
        let model_tokens = sub(self.model_tokens, rhs.model_tokens, BudgetDimension::ModelTokens);
        let model_tokens = match model_tokens {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let provider_cost_microunits = sub(
            self.provider_cost_microunits,
            rhs.provider_cost_microunits,
            BudgetDimension::ProviderCostMicrounits,
        );
        let provider_cost_microunits = match provider_cost_microunits {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let active_effect_milliseconds = sub(
            self.active_effect_milliseconds,
            rhs.active_effect_milliseconds,
            BudgetDimension::ActiveEffectMilliseconds,
        );
        let active_effect_milliseconds = match active_effect_milliseconds {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let attempts = sub(self.attempts, rhs.attempts, BudgetDimension::Attempts);
        let attempts = match attempts {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let retries = sub(self.retries, rhs.retries, BudgetDimension::Retries);
        let retries = match retries {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        Ok(Self::new(
            model_tokens,
            provider_cost_microunits,
            active_effect_milliseconds,
            attempts,
            retries,
        ))
    }
}
