//! Executable conservation helpers and their mathematical counterparts.

#![allow(
    clippy::missing_const_for_fn,
    clippy::too_many_lines,
    reason = "The executable conservation fold carries a line-by-line Verus postcondition proof"
)]

use crate::{BudgetAmounts, BudgetError, BudgetErrorKind};
#[cfg(verus_only)]
use crate::BudgetDimension;
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

/// Mathematical componentwise ordering used by the ledger proofs.
pub(crate) open spec fn amounts_le(left: BudgetAmounts, right: BudgetAmounts) -> bool {
    left.spec_le(right)
}

/// Mathematical conservation relation for one account.
pub(crate) open spec fn account_conserves(account: crate::state::BudgetAccount) -> bool {
    account_balance_holds(account, BudgetDimension::ModelTokens)
        && account_balance_holds(account, BudgetDimension::ProviderCostMicrounits)
        && account_balance_holds(account, BudgetDimension::ActiveEffectMilliseconds)
        && account_balance_holds(account, BudgetDimension::Attempts)
        && account_balance_holds(account, BudgetDimension::Retries)
}

pub(crate) open spec fn account_balance_holds(
    account: crate::state::BudgetAccount,
    dimension: BudgetDimension,
) -> bool {
    account.consumed.spec_get(dimension)
        + account.operation_reserved.spec_get(dimension)
        + account.child_delegated_remaining.spec_get(dimension)
        <= account.limits.spec_amounts().spec_get(dimension)
}

pub(crate) open spec fn available_is_exact(
    account: crate::state::BudgetAccount,
    available: BudgetAmounts,
) -> bool {
    account.consumed.spec_get(BudgetDimension::ModelTokens)
            + account.operation_reserved.spec_get(BudgetDimension::ModelTokens)
            + account.child_delegated_remaining.spec_get(BudgetDimension::ModelTokens)
            + available.spec_get(BudgetDimension::ModelTokens)
        == account.limits.spec_amounts().spec_get(BudgetDimension::ModelTokens)
        && account.consumed.spec_get(BudgetDimension::ProviderCostMicrounits)
            + account.operation_reserved.spec_get(BudgetDimension::ProviderCostMicrounits)
            + account.child_delegated_remaining.spec_get(BudgetDimension::ProviderCostMicrounits)
            + available.spec_get(BudgetDimension::ProviderCostMicrounits)
        == account.limits.spec_amounts().spec_get(BudgetDimension::ProviderCostMicrounits)
        && account.consumed.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            + account.operation_reserved.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            + account.child_delegated_remaining.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            + available.spec_get(BudgetDimension::ActiveEffectMilliseconds)
        == account.limits.spec_amounts().spec_get(BudgetDimension::ActiveEffectMilliseconds)
        && account.consumed.spec_get(BudgetDimension::Attempts)
            + account.operation_reserved.spec_get(BudgetDimension::Attempts)
            + account.child_delegated_remaining.spec_get(BudgetDimension::Attempts)
            + available.spec_get(BudgetDimension::Attempts)
        == account.limits.spec_amounts().spec_get(BudgetDimension::Attempts)
        && account.consumed.spec_get(BudgetDimension::Retries)
            + account.operation_reserved.spec_get(BudgetDimension::Retries)
            + account.child_delegated_remaining.spec_get(BudgetDimension::Retries)
            + available.spec_get(BudgetDimension::Retries)
        == account.limits.spec_amounts().spec_get(BudgetDimension::Retries)
}

/// Mathematical statement that authoritative consumption never decreases.
pub(crate) open spec fn consumption_monotonic(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
) -> bool {
    amounts_le(before.consumed, after.consumed)
}

/// Concrete conservation invariant over every account in a ledger value.
pub(crate) open spec fn ledger_conserves(ledger: &crate::BudgetLedger) -> bool {
    forall |index: int| #![auto]
        0 <= index < ledger.accounts@.len()
            ==> account_conserves(ledger.accounts[index])
}

/// Complete concrete ledger invariant used by accepted reducer postconditions.
pub(crate) open spec fn ledger_well_formed(ledger: &crate::BudgetLedger) -> bool {
    ledger_conserves(ledger) && crate::invariant::ledger_structure_holds(ledger)
}

/// Concrete prefix relation proving no existing account's consumption moved backward.
pub(crate) open spec fn ledger_consumption_monotonic(
    before: &crate::BudgetLedger,
    after: &crate::BudgetLedger,
) -> bool {
    before.accounts@.len() <= after.accounts@.len()
        && forall |index: int| #![auto]
            0 <= index < before.accounts@.len()
                ==> consumption_monotonic(before.accounts[index], after.accounts[index])
}

/// Concrete prefix relation proving accepted cumulative reservation observations never decrease.
pub(crate) open spec fn ledger_high_water_monotonic(
    before: &crate::BudgetLedger,
    after: &crate::BudgetLedger,
) -> bool {
    before.reservations@.len() <= after.reservations@.len()
        && forall |index: int| #![auto]
            0 <= index < before.reservations@.len()
                ==> amounts_le(
                        before.reservations[index].observed,
                        after.reservations[index].observed,
                    )
}

// Verus requires the visibility of this executable contract to match its crate-private predicates.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn available(
    account: &crate::state::BudgetAccount,
) -> (result: Result<BudgetAmounts, BudgetError>)
    ensures
        match result {
            Ok(available) => account_conserves(*account) && available_is_exact(*account, available),
            Err(error) => {
                !account_conserves(*account)
                    && crate::reachability::infrastructure_error(error)
            }
        },
{
    account.limits.amounts().establish_bounds();
    account.consumed.establish_bounds();
    account.operation_reserved.establish_bounds();
    account.child_delegated_remaining.establish_bounds();
    let after_consumed = match account.limits.amounts().checked_sub(account.consumed) {
        Ok(after_consumed) => after_consumed,
        Err(arithmetic) => {
            proof {
                assert(BudgetAmounts::subtraction_error_exact(
                    arithmetic,
                    account.limits.spec_amounts(),
                    account.consumed,
                ));
                match arithmetic.spec_dimension() {
                    BudgetDimension::ModelTokens => {
                        assert(!account_balance_holds(*account, BudgetDimension::ModelTokens));
                    }
                    BudgetDimension::ProviderCostMicrounits => {
                        assert(!account_balance_holds(
                            *account,
                            BudgetDimension::ProviderCostMicrounits,
                        ));
                    }
                    BudgetDimension::ActiveEffectMilliseconds => {
                        assert(!account_balance_holds(
                            *account,
                            BudgetDimension::ActiveEffectMilliseconds,
                        ));
                    }
                    BudgetDimension::Attempts => {
                        assert(!account_balance_holds(*account, BudgetDimension::Attempts));
                    }
                    BudgetDimension::Retries => {
                        assert(!account_balance_holds(*account, BudgetDimension::Retries));
                    }
                }
                assert(!account_conserves(*account));
            }
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    let after_reserved = match after_consumed.checked_sub(account.operation_reserved) {
        Ok(after_reserved) => after_reserved,
        Err(arithmetic) => {
            proof {
                assert(BudgetAmounts::spec_difference(
                    after_consumed,
                    account.limits.spec_amounts(),
                    account.consumed,
                ));
                assert(BudgetAmounts::subtraction_error_exact(
                    arithmetic,
                    after_consumed,
                    account.operation_reserved,
                ));
                match arithmetic.spec_dimension() {
                    BudgetDimension::ModelTokens => {
                        assert(!account_balance_holds(*account, BudgetDimension::ModelTokens));
                    }
                    BudgetDimension::ProviderCostMicrounits => {
                        assert(!account_balance_holds(
                            *account,
                            BudgetDimension::ProviderCostMicrounits,
                        ));
                    }
                    BudgetDimension::ActiveEffectMilliseconds => {
                        assert(!account_balance_holds(
                            *account,
                            BudgetDimension::ActiveEffectMilliseconds,
                        ));
                    }
                    BudgetDimension::Attempts => {
                        assert(!account_balance_holds(*account, BudgetDimension::Attempts));
                    }
                    BudgetDimension::Retries => {
                        assert(!account_balance_holds(*account, BudgetDimension::Retries));
                    }
                }
                assert(!account_conserves(*account));
            }
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    let available = match after_reserved.checked_sub(account.child_delegated_remaining) {
        Ok(available) => available,
        Err(arithmetic) => {
            proof {
                assert(BudgetAmounts::spec_difference(
                    after_consumed,
                    account.limits.spec_amounts(),
                    account.consumed,
                ));
                assert(BudgetAmounts::spec_difference(
                    after_reserved,
                    after_consumed,
                    account.operation_reserved,
                ));
                assert(BudgetAmounts::subtraction_error_exact(
                    arithmetic,
                    after_reserved,
                    account.child_delegated_remaining,
                ));
                match arithmetic.spec_dimension() {
                    BudgetDimension::ModelTokens => {
                        assert(!account_balance_holds(*account, BudgetDimension::ModelTokens));
                    }
                    BudgetDimension::ProviderCostMicrounits => {
                        assert(!account_balance_holds(
                            *account,
                            BudgetDimension::ProviderCostMicrounits,
                        ));
                    }
                    BudgetDimension::ActiveEffectMilliseconds => {
                        assert(!account_balance_holds(
                            *account,
                            BudgetDimension::ActiveEffectMilliseconds,
                        ));
                    }
                    BudgetDimension::Attempts => {
                        assert(!account_balance_holds(*account, BudgetDimension::Attempts));
                    }
                    BudgetDimension::Retries => {
                        assert(!account_balance_holds(*account, BudgetDimension::Retries));
                    }
                }
                assert(!account_conserves(*account));
            }
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    #[cfg(verus_only)]
    let model_tokens = available.get(BudgetDimension::ModelTokens).get();
    #[cfg(verus_only)]
    let provider_cost = available.get(BudgetDimension::ProviderCostMicrounits).get();
    #[cfg(verus_only)]
    let active_effect = available.get(BudgetDimension::ActiveEffectMilliseconds).get();
    #[cfg(verus_only)]
    let attempts = available.get(BudgetDimension::Attempts).get();
    #[cfg(verus_only)]
    let retries = available.get(BudgetDimension::Retries).get();
    #[cfg(verus_only)]
    let available = BudgetAmounts::from_units(
        model_tokens,
        provider_cost,
        active_effect,
        attempts,
        retries,
    );
    proof {
        assert(0 <= model_tokens);
        assert(0 <= provider_cost);
        assert(0 <= active_effect);
        assert(0 <= attempts);
        assert(0 <= retries);
        assert(account_balance_holds(*account, BudgetDimension::ModelTokens));
        assert(account_balance_holds(*account, BudgetDimension::ProviderCostMicrounits));
        assert(account_balance_holds(*account, BudgetDimension::ActiveEffectMilliseconds));
        assert(account_balance_holds(*account, BudgetDimension::Attempts));
        assert(account_balance_holds(*account, BudgetDimension::Retries));
    }
    Ok(available)
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module exact error contract"
)]
pub(crate) const fn corrupt(budget_id: BudgetId) -> (result: BudgetError)
    ensures
        crate::reachability::infrastructure_error(result),
        crate::reachability::exact_budget_error(
            result,
            BudgetErrorKind::CorruptState,
            budget_id,
        ),
{
    BudgetError::budget(BudgetErrorKind::CorruptState, budget_id)
}

} // verus!
