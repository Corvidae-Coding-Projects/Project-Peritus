//! Fixed-dimensional checked budget values.

mod operations;

use crate::{AmountArithmeticError, BudgetDimension};
use peritus_types::{ResourceQuantity, ResourceQuantityError};
use vstd::prelude::*;

verus! {

/// A complete nonnegative amount in every monotonic budget dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetAmounts {
    model_tokens: ResourceQuantity,
    provider_cost_microunits: ResourceQuantity,
    active_effect_milliseconds: ResourceQuantity,
    attempts: ResourceQuantity,
    retries: ResourceQuantity,
}

impl BudgetAmounts {
    /// Returns zero in every dimension.
    #[must_use]
    pub const fn zero() -> (result: Self)
        ensures result.spec_is_zero(),
    {
        Self::from_units(0, 0, 0, 0, 0)
    }

    /// Creates a complete amount from checked resource quantities.
    #[must_use]
    pub const fn new(
        model_tokens: ResourceQuantity,
        provider_cost_microunits: ResourceQuantity,
        active_effect_milliseconds: ResourceQuantity,
        attempts: ResourceQuantity,
        retries: ResourceQuantity,
    ) -> (result: Self)
        ensures
            result.spec_get(BudgetDimension::ModelTokens) == model_tokens.spec_value(),
            result.spec_get(BudgetDimension::ProviderCostMicrounits)
                == provider_cost_microunits.spec_value(),
            result.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                == active_effect_milliseconds.spec_value(),
            result.spec_get(BudgetDimension::Attempts) == attempts.spec_value(),
            result.spec_get(BudgetDimension::Retries) == retries.spec_value(),
    {
        Self {
            model_tokens,
            provider_cost_microunits,
            active_effect_milliseconds,
            attempts,
            retries,
        }
    }

    /// Creates a complete amount from primitive unit counts.
    #[must_use]
    pub const fn from_units(
        model_tokens: u64,
        provider_cost_microunits: u64,
        active_effect_milliseconds: u64,
        attempts: u64,
        retries: u64,
    ) -> (result: Self)
        ensures
            result.spec_get(BudgetDimension::ModelTokens) == model_tokens,
            result.spec_get(BudgetDimension::ProviderCostMicrounits)
                == provider_cost_microunits,
            result.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                == active_effect_milliseconds,
            result.spec_get(BudgetDimension::Attempts) == attempts,
            result.spec_get(BudgetDimension::Retries) == retries,
    {
        Self::new(
            ResourceQuantity::new(model_tokens),
            ResourceQuantity::new(provider_cost_microunits),
            ResourceQuantity::new(active_effect_milliseconds),
            ResourceQuantity::new(attempts),
            ResourceQuantity::new(retries),
        )
    }

    /// Returns the amount for one dimension.
    #[must_use]
    pub const fn get(
        self,
        dimension: BudgetDimension,
    ) -> (quantity: ResourceQuantity)
        ensures
            quantity.spec_value() == self.spec_get(dimension),
    {
        match dimension {
            BudgetDimension::ModelTokens => self.model_tokens,
            BudgetDimension::ProviderCostMicrounits => self.provider_cost_microunits,
            BudgetDimension::ActiveEffectMilliseconds => self.active_effect_milliseconds,
            BudgetDimension::Attempts => self.attempts,
            BudgetDimension::Retries => self.retries,
        }
    }

    /// Returns the mathematical value of one dimension.
    pub closed spec fn spec_get(&self, dimension: BudgetDimension) -> int {
        match dimension {
            BudgetDimension::ModelTokens => self.model_tokens.spec_value(),
            BudgetDimension::ProviderCostMicrounits => self.provider_cost_microunits.spec_value(),
            BudgetDimension::ActiveEffectMilliseconds => {
                self.active_effect_milliseconds.spec_value()
            }
            BudgetDimension::Attempts => self.attempts.spec_value(),
            BudgetDimension::Retries => self.retries.spec_value(),
        }
    }

    /// Exact first failing dimension for componentwise subtraction.
    pub open spec fn subtraction_error_exact(
        error: AmountArithmeticError,
        left: Self,
        right: Self,
    ) -> bool {
        error.spec_kind() == crate::ArithmeticKind::Underflow
            && left.spec_get(error.spec_dimension()) < right.spec_get(error.spec_dimension())
            && match error.spec_dimension() {
                BudgetDimension::ModelTokens => true,
                BudgetDimension::ProviderCostMicrounits => {
                    right.spec_get(BudgetDimension::ModelTokens)
                        <= left.spec_get(BudgetDimension::ModelTokens)
                }
                BudgetDimension::ActiveEffectMilliseconds => {
                    right.spec_get(BudgetDimension::ModelTokens)
                            <= left.spec_get(BudgetDimension::ModelTokens)
                        && right.spec_get(BudgetDimension::ProviderCostMicrounits)
                            <= left.spec_get(BudgetDimension::ProviderCostMicrounits)
                }
                BudgetDimension::Attempts => {
                    right.spec_get(BudgetDimension::ModelTokens)
                            <= left.spec_get(BudgetDimension::ModelTokens)
                        && right.spec_get(BudgetDimension::ProviderCostMicrounits)
                            <= left.spec_get(BudgetDimension::ProviderCostMicrounits)
                        && right.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                            <= left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                }
                BudgetDimension::Retries => {
                    right.spec_get(BudgetDimension::ModelTokens)
                            <= left.spec_get(BudgetDimension::ModelTokens)
                        && right.spec_get(BudgetDimension::ProviderCostMicrounits)
                            <= left.spec_get(BudgetDimension::ProviderCostMicrounits)
                        && right.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                            <= left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                        && right.spec_get(BudgetDimension::Attempts)
                            <= left.spec_get(BudgetDimension::Attempts)
                }
            }
    }

    /// Exact first failing dimension for componentwise addition.
    pub open spec fn addition_error_exact(
        error: AmountArithmeticError,
        left: Self,
        right: Self,
    ) -> bool {
        error.spec_kind() == crate::ArithmeticKind::Overflow
            && left.spec_get(error.spec_dimension()) + right.spec_get(error.spec_dimension())
                > u64::MAX
            && match error.spec_dimension() {
                BudgetDimension::ModelTokens => true,
                BudgetDimension::ProviderCostMicrounits => {
                    left.spec_get(BudgetDimension::ModelTokens)
                            + right.spec_get(BudgetDimension::ModelTokens) <= u64::MAX
                }
                BudgetDimension::ActiveEffectMilliseconds => {
                    left.spec_get(BudgetDimension::ModelTokens)
                            + right.spec_get(BudgetDimension::ModelTokens) <= u64::MAX
                        && left.spec_get(BudgetDimension::ProviderCostMicrounits)
                            + right.spec_get(BudgetDimension::ProviderCostMicrounits) <= u64::MAX
                }
                BudgetDimension::Attempts => {
                    left.spec_get(BudgetDimension::ModelTokens)
                            + right.spec_get(BudgetDimension::ModelTokens) <= u64::MAX
                        && left.spec_get(BudgetDimension::ProviderCostMicrounits)
                            + right.spec_get(BudgetDimension::ProviderCostMicrounits) <= u64::MAX
                        && left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                            + right.spec_get(BudgetDimension::ActiveEffectMilliseconds) <= u64::MAX
                }
                BudgetDimension::Retries => {
                    left.spec_get(BudgetDimension::ModelTokens)
                            + right.spec_get(BudgetDimension::ModelTokens) <= u64::MAX
                        && left.spec_get(BudgetDimension::ProviderCostMicrounits)
                            + right.spec_get(BudgetDimension::ProviderCostMicrounits) <= u64::MAX
                        && left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                            + right.spec_get(BudgetDimension::ActiveEffectMilliseconds) <= u64::MAX
                        && left.spec_get(BudgetDimension::Attempts)
                            + right.spec_get(BudgetDimension::Attempts) <= u64::MAX
                }
            }
    }

    /// Whether componentwise addition is not representable in at least one dimension.
    pub open spec fn spec_addition_overflows(left: Self, right: Self) -> bool {
        left.spec_get(BudgetDimension::ModelTokens)
                + right.spec_get(BudgetDimension::ModelTokens) > u64::MAX
            || left.spec_get(BudgetDimension::ProviderCostMicrounits)
                + right.spec_get(BudgetDimension::ProviderCostMicrounits) > u64::MAX
            || left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                + right.spec_get(BudgetDimension::ActiveEffectMilliseconds) > u64::MAX
            || left.spec_get(BudgetDimension::Attempts)
                + right.spec_get(BudgetDimension::Attempts) > u64::MAX
            || left.spec_get(BudgetDimension::Retries)
                + right.spec_get(BudgetDimension::Retries) > u64::MAX
    }

    pub(crate) const fn establish_bounds(self)
        ensures
            0 <= self.spec_get(BudgetDimension::ModelTokens)
                <= u64::MAX,
            0 <= self.spec_get(BudgetDimension::ProviderCostMicrounits)
                <= u64::MAX,
            0 <= self.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                <= u64::MAX,
            0 <= self.spec_get(BudgetDimension::Attempts)
                <= u64::MAX,
            0 <= self.spec_get(BudgetDimension::Retries)
                <= u64::MAX,
    {
        let _ = self.model_tokens.get();
        let _ = self.provider_cost_microunits.get();
        let _ = self.active_effect_milliseconds.get();
        let _ = self.attempts.get();
        let _ = self.retries.get();
    }

    pub(crate) const fn zero_sum_has_zero_operands(result: Self, left: Self, right: Self)
        requires
            Self::spec_sum(result, left, right),
            result.spec_is_zero(),
        ensures
            left.spec_is_zero(),
            right.spec_is_zero(),
    {
        let _ = result.is_zero();
        left.establish_bounds();
        right.establish_bounds();
    }

    pub(crate) const fn difference_le_left(result: Self, left: Self, right: Self)
        requires Self::spec_difference(result, left, right),
        ensures result.spec_le(left),
    {
        result.establish_bounds();
        left.establish_bounds();
        right.establish_bounds();
    }

    /// Returns whether every mathematical dimension is zero.
    pub open spec fn spec_is_zero(&self) -> bool {
        self.spec_get(BudgetDimension::ModelTokens) == 0
            && self.spec_get(BudgetDimension::ProviderCostMicrounits) == 0
            && self.spec_get(BudgetDimension::ActiveEffectMilliseconds) == 0
            && self.spec_get(BudgetDimension::Attempts) == 0
            && self.spec_get(BudgetDimension::Retries) == 0
    }

    /// Componentwise mathematical ordering.
    pub open spec fn spec_le(&self, ceiling: Self) -> bool {
        self.spec_get(BudgetDimension::ModelTokens)
                <= ceiling.spec_get(BudgetDimension::ModelTokens)
            && self.spec_get(BudgetDimension::ProviderCostMicrounits)
                <= ceiling.spec_get(BudgetDimension::ProviderCostMicrounits)
            && self.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                <= ceiling.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            && self.spec_get(BudgetDimension::Attempts)
                <= ceiling.spec_get(BudgetDimension::Attempts)
            && self.spec_get(BudgetDimension::Retries)
                <= ceiling.spec_get(BudgetDimension::Retries)
    }

    pub(crate) open spec fn spec_equal(&self, other: Self) -> bool {
        self.spec_get(BudgetDimension::ModelTokens)
                == other.spec_get(BudgetDimension::ModelTokens)
            && self.spec_get(BudgetDimension::ProviderCostMicrounits)
                == other.spec_get(BudgetDimension::ProviderCostMicrounits)
            && self.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                == other.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            && self.spec_get(BudgetDimension::Attempts)
                == other.spec_get(BudgetDimension::Attempts)
            && self.spec_get(BudgetDimension::Retries)
                == other.spec_get(BudgetDimension::Retries)
    }

    /// Exact componentwise mathematical addition relation.
    pub open spec fn spec_sum(result: Self, left: Self, right: Self) -> bool {
        result.spec_get(BudgetDimension::ModelTokens)
                == left.spec_get(BudgetDimension::ModelTokens)
                    + right.spec_get(BudgetDimension::ModelTokens)
            && result.spec_get(BudgetDimension::ProviderCostMicrounits)
                == left.spec_get(BudgetDimension::ProviderCostMicrounits)
                    + right.spec_get(BudgetDimension::ProviderCostMicrounits)
            && result.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                == left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                    + right.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            && result.spec_get(BudgetDimension::Attempts)
                == left.spec_get(BudgetDimension::Attempts)
                    + right.spec_get(BudgetDimension::Attempts)
            && result.spec_get(BudgetDimension::Retries)
                == left.spec_get(BudgetDimension::Retries)
                    + right.spec_get(BudgetDimension::Retries)
    }

    /// Exact componentwise mathematical subtraction relation.
    pub open spec fn spec_difference(result: Self, left: Self, right: Self) -> bool {
        result.spec_get(BudgetDimension::ModelTokens)
                == left.spec_get(BudgetDimension::ModelTokens)
                    - right.spec_get(BudgetDimension::ModelTokens)
            && result.spec_get(BudgetDimension::ProviderCostMicrounits)
                == left.spec_get(BudgetDimension::ProviderCostMicrounits)
                    - right.spec_get(BudgetDimension::ProviderCostMicrounits)
            && result.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                == left.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                    - right.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            && result.spec_get(BudgetDimension::Attempts)
                == left.spec_get(BudgetDimension::Attempts)
                    - right.spec_get(BudgetDimension::Attempts)
            && result.spec_get(BudgetDimension::Retries)
                == left.spec_get(BudgetDimension::Retries)
                    - right.spec_get(BudgetDimension::Retries)
    }
}

const fn add(
    left: ResourceQuantity,
    right: ResourceQuantity,
    dimension: BudgetDimension,
) -> (result: Result<ResourceQuantity, AmountArithmeticError>)
    ensures
        match result {
                Ok(sum) => {
                    sum.spec_value() == left.spec_value() + right.spec_value()
                        && left.spec_value() + right.spec_value() <= u64::MAX
                }
            Err(error) => {
                error.spec_kind() == crate::ArithmeticKind::Overflow
                    && error.spec_dimension() == dimension
                    && left.spec_value() + right.spec_value() > u64::MAX
            }
        },
{
    match left.checked_add(right) {
        Ok(value) => {
            let raw = value.get();
            assert(raw <= u64::MAX);
            assert(value.spec_value() == raw);
            Ok(value)
        }
        Err(ResourceQuantityError::Overflow) => Err(AmountArithmeticError::overflow(dimension)),
        Err(ResourceQuantityError::Underflow) => {
            Err(AmountArithmeticError::underflow(dimension))
        }
    }
}

const fn sub(
    left: ResourceQuantity,
    right: ResourceQuantity,
    dimension: BudgetDimension,
) -> (result: Result<ResourceQuantity, AmountArithmeticError>)
    ensures
        match result {
                Ok(difference) => {
                    difference.spec_value() == left.spec_value() - right.spec_value()
                        && right.spec_value() <= left.spec_value()
                }
            Err(error) => {
                error.spec_kind() == crate::ArithmeticKind::Underflow
                    && error.spec_dimension() == dimension
                    && left.spec_value() < right.spec_value()
            }
        },
{
    match left.checked_sub(right) {
        Ok(value) => {
            let raw = value.get();
            assert(raw >= 0);
            assert(value.spec_value() == raw);
            Ok(value)
        }
        Err(ResourceQuantityError::Underflow) => Err(AmountArithmeticError::underflow(dimension)),
        Err(ResourceQuantityError::Overflow) => {
            Err(AmountArithmeticError::overflow(dimension))
        }
    }
}

} // verus!
