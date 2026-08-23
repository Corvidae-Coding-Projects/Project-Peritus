//! Executable exact folds for delegated and operation-reserved capacity.

mod reservations;
mod support;

pub(super) use self::reservations::direct_operation_reserved;
use self::support::{account_at, child_contributes};
#[cfg(verus_only)]
use self::support::{bind_child_remaining, bind_child_zero};
use crate::{BudgetAmounts, BudgetError, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(super) fn direct_child_remaining(
    ledger: &BudgetLedger,
    account_index: usize,
) -> (result: Result<BudgetAmounts, BudgetError>)
    ensures
        (crate::model::ledger_well_formed(ledger)
            && account_index < ledger.accounts@.len()) ==> result.is_ok(),
        match result {
            Ok(total) => crate::accounting_model::amount_matches_child_sum(
                total,
                ledger,
                ledger.accounts[account_index as int].id,
                ledger.accounts@.len() as int,
            ),
            Err(_) => true,
        },
{
    if account_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    assert(account_index < ledger.accounts@.len());
    let account = ledger.accounts[account_index];
    account.child_delegated_remaining.establish_bounds();
    let mut total = BudgetAmounts::zero();
    let mut index = 0;
    while index < ledger.accounts.len()
        invariant
            0 <= index <= ledger.accounts.len(),
            account_index < ledger.accounts@.len(),
            account.id == ledger.accounts[account_index as int].id,
            account.revision == ledger.accounts[account_index as int].revision,
            crate::accounting_model::amount_matches_child_sum(
                total, ledger, account.id, index as int,
            ),
            crate::model::ledger_well_formed(ledger)
                && account_index < ledger.accounts@.len()
                ==> total.spec_le(ledger.accounts[account_index as int].child_delegated_remaining),
        decreases ledger.accounts.len() - index,
    {
        let child = account_at(ledger, index);
        let contributes = child_contributes(&child, account.id);
        if contributes {
            if !crate::identity_model::revision_equal(child.revision, account.revision) {
                proof {
                    if crate::model::ledger_well_formed(ledger) {
                        assert(crate::invariant::ledger_structure_holds(ledger));
                        assert(crate::invariant::ledger_account_structure_holds(ledger));
                        assert(crate::invariant::account_entry_valid(ledger, index as int));
                        assert(crate::invariant::parent_link_valid(ledger, index as int));
                        let parent = choose |parent: int| #![auto]
                            0 <= parent < index
                                && crate::identity_model::parent_matches(
                                    ledger.accounts[index as int].parent_id,
                                    ledger.accounts[parent].id,
                                )
                                && crate::identity_model::revisions_equal(
                                    ledger.accounts[index as int].revision,
                                    ledger.accounts[parent].revision,
                                );
                        assert(crate::identity_model::parent_matches(
                            ledger.accounts[index as int].parent_id,
                            ledger.accounts[account_index as int].id,
                        ));
                        assert(crate::identity_model::budget_ids_equal(
                            ledger.accounts[parent].id,
                            ledger.accounts[account_index as int].id,
                        ));
                        crate::invariant::matching_accounts_are_unique(
                            ledger,
                            parent,
                            account_index as int,
                        );
                        assert(parent == account_index as int);
                    }
                }
                return Err(crate::model::corrupt(child.id));
            }
            child.limits.amounts().establish_bounds();
            child.consumed.establish_bounds();
            child.operation_reserved.establish_bounds();
            child.child_delegated_remaining.establish_bounds();
            let remaining = match child.limits.amounts().checked_sub(child.consumed) {
                Ok(remaining) => remaining,
                Err(error) => {
                    proof {
                        if crate::model::ledger_well_formed(ledger) {
                            assert(crate::model::ledger_conserves(ledger));
                            assert(crate::model::account_conserves(
                                ledger.accounts[index as int],
                            ));
                            assert(crate::model::account_balance_holds(
                                child,
                                crate::BudgetDimension::ModelTokens,
                            ));
                            assert(crate::model::account_balance_holds(
                                child,
                                crate::BudgetDimension::ProviderCostMicrounits,
                            ));
                            assert(crate::model::account_balance_holds(
                                child,
                                crate::BudgetDimension::ActiveEffectMilliseconds,
                            ));
                            assert(crate::model::account_balance_holds(
                                child,
                                crate::BudgetDimension::Attempts,
                            ));
                            assert(crate::model::account_balance_holds(
                                child,
                                crate::BudgetDimension::Retries,
                            ));
                            assert(child.consumed.spec_get(crate::BudgetDimension::ModelTokens)
                                <= child.limits.spec_amounts().spec_get(
                                    crate::BudgetDimension::ModelTokens,
                                ));
                            assert(child.consumed.spec_get(
                                    crate::BudgetDimension::ProviderCostMicrounits,
                                ) <= child.limits.spec_amounts().spec_get(
                                    crate::BudgetDimension::ProviderCostMicrounits,
                                ));
                            assert(child.consumed.spec_get(
                                    crate::BudgetDimension::ActiveEffectMilliseconds,
                                ) <= child.limits.spec_amounts().spec_get(
                                    crate::BudgetDimension::ActiveEffectMilliseconds,
                                ));
                            assert(child.consumed.spec_get(crate::BudgetDimension::Attempts)
                                <= child.limits.spec_amounts().spec_get(
                                    crate::BudgetDimension::Attempts,
                                ));
                            assert(child.consumed.spec_get(crate::BudgetDimension::Retries)
                                <= child.limits.spec_amounts().spec_get(
                                    crate::BudgetDimension::Retries,
                                ));
                            assert(child.consumed.spec_le(child.limits.spec_amounts()));
                            super::arithmetic::subtraction_error_impossible(
                                error,
                                child.limits.spec_amounts(),
                                child.consumed,
                            );
                        }
                    }
                    return Err(crate::model::corrupt(child.id));
                }
            };
            proof {
                assert(crate::identity_model::parent_matches(
                    ledger.accounts[index as int].parent_id,
                    account.id,
                ));
                assert(crate::accounting_model::account_not_closed(
                    ledger.accounts[index as int].phase,
                ));
                bind_child_remaining(ledger, account.id, index as int, remaining);
                crate::accounting_model::child_sum_step(
                    ledger,
                    account.id,
                    crate::BudgetDimension::ModelTokens,
                    index as int,
                );
                crate::accounting_model::child_sum_step(
                    ledger,
                    account.id,
                    crate::BudgetDimension::ProviderCostMicrounits,
                    index as int,
                );
                crate::accounting_model::child_sum_step(
                    ledger,
                    account.id,
                    crate::BudgetDimension::ActiveEffectMilliseconds,
                    index as int,
                );
                crate::accounting_model::child_sum_step(
                    ledger,
                    account.id,
                    crate::BudgetDimension::Attempts,
                    index as int,
                );
                crate::accounting_model::child_sum_step(
                    ledger,
                    account.id,
                    crate::BudgetDimension::Retries,
                    index as int,
                );
            }
            crate::accounting_prefix_model::child_prefix_le_account(
                ledger,
                account_index,
                (index + 1) as usize,
            );
            ledger.accounts[account_index].child_delegated_remaining.establish_bounds();
            let next = match total.checked_add(remaining) {
                Ok(sum) => sum,
                Err(error) => {
                    proof {
                        if crate::model::ledger_well_formed(ledger) {
                            assert(total.spec_get(crate::BudgetDimension::ModelTokens)
                                    + remaining.spec_get(crate::BudgetDimension::ModelTokens)
                                <= ledger.accounts[account_index as int]
                                    .child_delegated_remaining
                                    .spec_get(crate::BudgetDimension::ModelTokens));
                            assert(total.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                                    + remaining.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                                <= ledger.accounts[account_index as int]
                                    .child_delegated_remaining
                                    .spec_get(crate::BudgetDimension::ProviderCostMicrounits));
                            assert(total.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                                    + remaining.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                                <= ledger.accounts[account_index as int]
                                    .child_delegated_remaining
                                    .spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
                            assert(total.spec_get(crate::BudgetDimension::Attempts)
                                    + remaining.spec_get(crate::BudgetDimension::Attempts)
                                <= ledger.accounts[account_index as int]
                                    .child_delegated_remaining
                                    .spec_get(crate::BudgetDimension::Attempts));
                            assert(total.spec_get(crate::BudgetDimension::Retries)
                                    + remaining.spec_get(crate::BudgetDimension::Retries)
                                <= ledger.accounts[account_index as int]
                                    .child_delegated_remaining
                                    .spec_get(crate::BudgetDimension::Retries));
                            super::arithmetic::addition_error_impossible(
                                error,
                                total,
                                remaining,
                                ledger.accounts[account_index as int]
                                    .child_delegated_remaining,
                            );
                        }
                    }
                    return Err(crate::model::corrupt(account.id));
                }
            };
            proof {
                assert(crate::identity_model::parent_matches(
                    ledger.accounts[index as int].parent_id,
                    account.id,
                ));
                assert(crate::accounting_model::account_not_closed(
                    ledger.accounts[index as int].phase,
                ));
                assert(BudgetAmounts::spec_difference(
                    remaining, child.limits.spec_amounts(), child.consumed,
                ));
                assert(BudgetAmounts::spec_sum(next, total, remaining));
                crate::accounting_model::advance_child_total(
                    ledger, account.id, index as int, total, remaining, next,
                );
                if crate::model::ledger_well_formed(ledger) {
                    assert(next.spec_get(crate::BudgetDimension::ModelTokens)
                        <= ledger.accounts[account_index as int]
                            .child_delegated_remaining
                            .spec_get(crate::BudgetDimension::ModelTokens));
                    assert(next.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                        <= ledger.accounts[account_index as int]
                            .child_delegated_remaining
                            .spec_get(crate::BudgetDimension::ProviderCostMicrounits));
                    assert(next.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                        <= ledger.accounts[account_index as int]
                            .child_delegated_remaining
                            .spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
                    assert(next.spec_get(crate::BudgetDimension::Attempts)
                        <= ledger.accounts[account_index as int]
                            .child_delegated_remaining
                            .spec_get(crate::BudgetDimension::Attempts));
                    assert(next.spec_get(crate::BudgetDimension::Retries)
                        <= ledger.accounts[account_index as int]
                            .child_delegated_remaining
                            .spec_get(crate::BudgetDimension::Retries));
                    assert(next.spec_le(
                        ledger.accounts[account_index as int].child_delegated_remaining,
                    ));
                }
            };
            total = next;
        } else {
            proof {
                assert(!(crate::identity_model::parent_matches(
                        ledger.accounts[index as int].parent_id,
                        account.id,
                    ) && crate::accounting_model::account_not_closed(
                        ledger.accounts[index as int].phase,
                    )));
                bind_child_zero(ledger, account.id, index as int);
                crate::accounting_model::advance_child_zero(
                    ledger, account.id, index as int, total,
                );
            };
        }
        index += 1;
    }
    Ok(total)
}

} // verus!
