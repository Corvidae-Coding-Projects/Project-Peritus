//! Contradictions that make derived-accounting arithmetic failures unreachable.

use crate::{AmountArithmeticError, BudgetAmounts};
use vstd::prelude::*;

verus! {

pub(super) proof fn subtraction_error_impossible(
    error: AmountArithmeticError,
    left: BudgetAmounts,
    right: BudgetAmounts,
)
    requires
        BudgetAmounts::subtraction_error_exact(error, left, right),
        right.spec_le(left),
    ensures false,
{
    match error.spec_dimension() {
        crate::BudgetDimension::ModelTokens => {}
        crate::BudgetDimension::ProviderCostMicrounits => {}
        crate::BudgetDimension::ActiveEffectMilliseconds => {}
        crate::BudgetDimension::Attempts => {}
        crate::BudgetDimension::Retries => {}
    }
}

pub(super) proof fn addition_error_impossible(
    error: AmountArithmeticError,
    left: BudgetAmounts,
    right: BudgetAmounts,
    bound: BudgetAmounts,
)
    requires
        BudgetAmounts::addition_error_exact(error, left, right),
        left.spec_get(crate::BudgetDimension::ModelTokens)
                + right.spec_get(crate::BudgetDimension::ModelTokens)
            <= bound.spec_get(crate::BudgetDimension::ModelTokens),
        left.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                + right.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            <= bound.spec_get(crate::BudgetDimension::ProviderCostMicrounits),
        left.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                + right.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            <= bound.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds),
        left.spec_get(crate::BudgetDimension::Attempts)
                + right.spec_get(crate::BudgetDimension::Attempts)
            <= bound.spec_get(crate::BudgetDimension::Attempts),
        left.spec_get(crate::BudgetDimension::Retries)
                + right.spec_get(crate::BudgetDimension::Retries)
            <= bound.spec_get(crate::BudgetDimension::Retries),
        bound.spec_get(crate::BudgetDimension::ModelTokens) <= u64::MAX,
        bound.spec_get(crate::BudgetDimension::ProviderCostMicrounits) <= u64::MAX,
        bound.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds) <= u64::MAX,
        bound.spec_get(crate::BudgetDimension::Attempts) <= u64::MAX,
        bound.spec_get(crate::BudgetDimension::Retries) <= u64::MAX,
    ensures false,
{
    match error.spec_dimension() {
        crate::BudgetDimension::ModelTokens => {}
        crate::BudgetDimension::ProviderCostMicrounits => {}
        crate::BudgetDimension::ActiveEffectMilliseconds => {}
        crate::BudgetDimension::Attempts => {}
        crate::BudgetDimension::Retries => {}
    }
}

} // verus!
