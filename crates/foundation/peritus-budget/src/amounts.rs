//! Fixed-dimensional checked budget values.

mod operations;

use crate::{AmountArithmeticError, BudgetDimension};
use peritus_types::{ResourceQuantity, ResourceQuantityError};

#[doc = " A complete nonnegative amount in every monotonic budget dimension."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetAmounts {
    model_tokens: ResourceQuantity,
    provider_cost_microunits: ResourceQuantity,
    active_effect_milliseconds: ResourceQuantity,
    attempts: ResourceQuantity,
    retries: ResourceQuantity,
}
impl BudgetAmounts {
    #[doc = " Returns zero in every dimension."]
    #[must_use]
    pub const fn zero() -> Self {
        Self::from_units(0, 0, 0, 0, 0)
    }
    #[doc = " Creates a complete amount from checked resource quantities."]
    #[must_use]
    pub const fn new(
        model_tokens: ResourceQuantity,
        provider_cost_microunits: ResourceQuantity,
        active_effect_milliseconds: ResourceQuantity,
        attempts: ResourceQuantity,
        retries: ResourceQuantity,
    ) -> Self {
        Self {
            model_tokens,
            provider_cost_microunits,
            active_effect_milliseconds,
            attempts,
            retries,
        }
    }
    #[doc = " Creates a complete amount from primitive unit counts."]
    #[must_use]
    pub const fn from_units(
        model_tokens: u64,
        provider_cost_microunits: u64,
        active_effect_milliseconds: u64,
        attempts: u64,
        retries: u64,
    ) -> Self {
        Self::new(
            ResourceQuantity::new(model_tokens),
            ResourceQuantity::new(provider_cost_microunits),
            ResourceQuantity::new(active_effect_milliseconds),
            ResourceQuantity::new(attempts),
            ResourceQuantity::new(retries),
        )
    }
    #[doc = " Returns the amount for one dimension."]
    #[must_use]
    pub const fn get(self, dimension: BudgetDimension) -> ResourceQuantity {
        match dimension {
            BudgetDimension::ModelTokens => self.model_tokens,
            BudgetDimension::ProviderCostMicrounits => self.provider_cost_microunits,
            BudgetDimension::ActiveEffectMilliseconds => self.active_effect_milliseconds,
            BudgetDimension::Attempts => self.attempts,
            BudgetDimension::Retries => self.retries,
        }
    }
    pub(crate) const fn establish_bounds(self) {
        let _ = self.model_tokens.get();
        let _ = self.provider_cost_microunits.get();
        let _ = self.active_effect_milliseconds.get();
        let _ = self.attempts.get();
        let _ = self.retries.get();
    }
    pub(crate) const fn zero_sum_has_zero_operands(result: Self, left: Self, right: Self) {
        let _ = result.is_zero();
        left.establish_bounds();
        right.establish_bounds();
    }
    pub(crate) const fn difference_le_left(result: Self, left: Self, right: Self) {
        result.establish_bounds();
        left.establish_bounds();
        right.establish_bounds();
    }
}
#[allow(unused_variables, reason = "Verus scalar witness")]
const fn add(
    left: ResourceQuantity,
    right: ResourceQuantity,
    dimension: BudgetDimension,
) -> Result<ResourceQuantity, AmountArithmeticError> {
    match left.checked_add(right) {
        Ok(value) => {
            let raw = value.get();
            Ok(value)
        }
        Err(ResourceQuantityError::Overflow) => Err(AmountArithmeticError::overflow(dimension)),
        Err(ResourceQuantityError::Underflow) => Err(AmountArithmeticError::underflow(dimension)),
    }
}
#[allow(unused_variables, reason = "Verus scalar witness")]
const fn sub(
    left: ResourceQuantity,
    right: ResourceQuantity,
    dimension: BudgetDimension,
) -> Result<ResourceQuantity, AmountArithmeticError> {
    match left.checked_sub(right) {
        Ok(value) => {
            let raw = value.get();
            Ok(value)
        }
        Err(ResourceQuantityError::Underflow) => Err(AmountArithmeticError::underflow(dimension)),
        Err(ResourceQuantityError::Overflow) => Err(AmountArithmeticError::overflow(dimension)),
    }
}
