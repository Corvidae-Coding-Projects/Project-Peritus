//! Exact fold for live operation-reserved capacity.

use super::super::super::accounting::outstanding;
use super::support::{reservation_at, reservation_contributes};
#[cfg(verus_only)]
use super::support::{bind_reservation_amount, bind_reservation_zero};
use crate::{BudgetAmounts, BudgetError, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(in crate::transition::validation) fn direct_operation_reserved(
    ledger: &BudgetLedger,
    account_index: usize,
) -> (result: Result<BudgetAmounts, BudgetError>)
    ensures
        (crate::model::ledger_well_formed(ledger)
            && account_index < ledger.accounts@.len()) ==> result.is_ok(),
        match result {
            Ok(total) => crate::accounting_model::amount_matches_reservation_sum(
                total,
                ledger,
                ledger.accounts[account_index as int].id,
                ledger.reservations@.len() as int,
            ),
            Err(_) => true,
        },
{
    if account_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    assert(account_index < ledger.accounts@.len());
    let budget_id = ledger.accounts[account_index].id;
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    let mut total = BudgetAmounts::zero();
    let mut index = 0;
    while index < ledger.reservations.len()
        invariant
            0 <= index <= ledger.reservations.len(),
            account_index < ledger.accounts@.len(),
            budget_id == ledger.accounts[account_index as int].id,
            crate::accounting_model::amount_matches_reservation_sum(
                total, ledger, budget_id, index as int,
            ),
            crate::model::ledger_well_formed(ledger)
                && account_index < ledger.accounts@.len()
                ==> total.spec_le(ledger.accounts[account_index as int].operation_reserved),
        decreases ledger.reservations.len() - index,
    {
        let reservation = reservation_at(ledger, index);
        let contributes = reservation_contributes(&reservation, budget_id);
        if contributes {
            let amount = match outstanding(&reservation) {
                Ok(amount) => amount,
                Err(_error) => {
                    proof {
                        if crate::model::ledger_well_formed(ledger) {
                            assert(crate::invariant::ledger_structure_holds(ledger));
                            assert(crate::invariant::ledger_reservation_structure_holds(ledger));
                            assert(crate::invariant::reservation_entry_valid(
                                ledger,
                                index as int,
                            ));
                        }
                    }
                    return Err(crate::model::corrupt(budget_id));
                }
            };
            proof {
                bind_reservation_amount(ledger, budget_id, index as int, amount);
                crate::accounting_model::reservation_sum_step(
                    ledger,
                    budget_id,
                    crate::BudgetDimension::ModelTokens,
                    index as int,
                );
                crate::accounting_model::reservation_sum_step(
                    ledger,
                    budget_id,
                    crate::BudgetDimension::ProviderCostMicrounits,
                    index as int,
                );
                crate::accounting_model::reservation_sum_step(
                    ledger,
                    budget_id,
                    crate::BudgetDimension::ActiveEffectMilliseconds,
                    index as int,
                );
                crate::accounting_model::reservation_sum_step(
                    ledger,
                    budget_id,
                    crate::BudgetDimension::Attempts,
                    index as int,
                );
                crate::accounting_model::reservation_sum_step(
                    ledger,
                    budget_id,
                    crate::BudgetDimension::Retries,
                    index as int,
                );
            }
            crate::accounting_prefix_model::reservation_prefix_le_account(
                ledger,
                account_index,
                (index + 1) as usize,
            );
            ledger.accounts[account_index].operation_reserved.establish_bounds();
            let next = match total.checked_add(amount) {
                Ok(sum) => sum,
                Err(error) => {
                    proof {
                        if crate::model::ledger_well_formed(ledger) {
                            assert(total.spec_get(crate::BudgetDimension::ModelTokens)
                                    + amount.spec_get(crate::BudgetDimension::ModelTokens)
                                <= ledger.accounts[account_index as int]
                                    .operation_reserved
                                    .spec_get(crate::BudgetDimension::ModelTokens));
                            assert(total.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                                    + amount.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                                <= ledger.accounts[account_index as int]
                                    .operation_reserved
                                    .spec_get(crate::BudgetDimension::ProviderCostMicrounits));
                            assert(total.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                                    + amount.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                                <= ledger.accounts[account_index as int]
                                    .operation_reserved
                                    .spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
                            assert(total.spec_get(crate::BudgetDimension::Attempts)
                                    + amount.spec_get(crate::BudgetDimension::Attempts)
                                <= ledger.accounts[account_index as int]
                                    .operation_reserved
                                    .spec_get(crate::BudgetDimension::Attempts));
                            assert(total.spec_get(crate::BudgetDimension::Retries)
                                    + amount.spec_get(crate::BudgetDimension::Retries)
                                <= ledger.accounts[account_index as int]
                                    .operation_reserved
                                    .spec_get(crate::BudgetDimension::Retries));
                            super::super::arithmetic::addition_error_impossible(
                                error,
                                total,
                                amount,
                                ledger.accounts[account_index as int].operation_reserved,
                            );
                        }
                    }
                    return Err(crate::model::corrupt(budget_id));
                }
            };
            proof {
                assert(crate::identity_model::budget_ids_equal(
                    ledger.reservations[index as int].request.spec_budget_id(),
                    budget_id,
                ));
                assert(BudgetAmounts::spec_sum(next, total, amount));
                crate::accounting_model::advance_reservation_total(
                    ledger, budget_id, index as int, total, amount, next,
                );
                if crate::model::ledger_well_formed(ledger) {
                    assert(next.spec_get(crate::BudgetDimension::ModelTokens)
                        <= ledger.accounts[account_index as int]
                            .operation_reserved
                            .spec_get(crate::BudgetDimension::ModelTokens));
                    assert(next.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                        <= ledger.accounts[account_index as int]
                            .operation_reserved
                            .spec_get(crate::BudgetDimension::ProviderCostMicrounits));
                    assert(next.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                        <= ledger.accounts[account_index as int]
                            .operation_reserved
                            .spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
                    assert(next.spec_get(crate::BudgetDimension::Attempts)
                        <= ledger.accounts[account_index as int]
                            .operation_reserved
                            .spec_get(crate::BudgetDimension::Attempts));
                    assert(next.spec_get(crate::BudgetDimension::Retries)
                        <= ledger.accounts[account_index as int]
                            .operation_reserved
                            .spec_get(crate::BudgetDimension::Retries));
                    assert(next.spec_le(
                        ledger.accounts[account_index as int].operation_reserved,
                    ));
                }
            };
            total = next;
        } else {
            proof {
                assert(!crate::identity_model::budget_ids_equal(
                    ledger.reservations[index as int].request.spec_budget_id(),
                    budget_id,
                ));
                bind_reservation_zero(ledger, budget_id, index as int);
                crate::accounting_model::advance_reservation_zero(
                    ledger, budget_id, index as int, total,
                );
            };
        }
        index += 1;
    }
    Ok(total)
}

} // verus!
