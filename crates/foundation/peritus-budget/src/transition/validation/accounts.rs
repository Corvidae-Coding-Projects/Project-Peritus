//! Complete executable account-tree and conservation validation.

use super::super::accounting::{find_account, has_live_work};
use crate::{BudgetError, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(super) fn validate(ledger: &BudgetLedger) -> (result: Result<(), BudgetError>)
    ensures
        crate::model::ledger_well_formed(ledger) ==> result.is_ok(),
        match result {
            Ok(()) => {
                crate::model::ledger_conserves(ledger)
                    && crate::invariant::ledger_account_structure_holds(ledger)
            }
            Err(_) => true,
        },
{
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::model::ledger_conserves(ledger));
            assert(crate::invariant::ledger_structure_holds(ledger));
            assert(crate::invariant::ledger_account_structure_holds(ledger));
            assert(ledger.accounts@.len() > 0);
        }
    }
    if ledger.accounts.is_empty() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let root_index = find_account(ledger, ledger.root_id)
        .ok_or_else(|| crate::model::corrupt(ledger.root_id))?;
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::identity_model::budget_ids_equal(
                ledger.accounts[0].id,
                ledger.root_id,
            ));
            crate::invariant::matching_accounts_are_unique(
                ledger,
                root_index as int,
                0,
            );
            assert(root_index == 0);
        }
    }
    if root_index != 0 || ledger.accounts[root_index].parent_id.is_some() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    assert(crate::identity_model::budget_ids_equal(
        ledger.accounts[0].id,
        ledger.root_id,
    ));

    let mut index = 0;
    while index < ledger.accounts.len()
        invariant
            0 <= index <= ledger.accounts.len(),
            ledger.accounts@.len() > 0,
            crate::identity_model::budget_ids_equal(
                ledger.accounts[0].id,
                ledger.root_id,
            ),
            ledger.accounts[0].parent_id.is_none(),
            crate::model::ledger_well_formed(ledger) ==> (
                crate::model::ledger_conserves(ledger)
                    && crate::invariant::ledger_account_structure_holds(ledger)
            ),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::model::account_conserves(ledger.accounts[checked]),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::invariant::account_entry_valid(ledger, checked),
        decreases ledger.accounts.len() - index,
    {
        let account = ledger.accounts[index];
        proof {
            if crate::model::ledger_well_formed(ledger) {
                assert(crate::invariant::account_entry_valid(ledger, index as int));
                assert(crate::model::account_conserves(ledger.accounts[index as int]));
            }
        }
        super::structure::validate_account_unique_before(ledger, index)?;
        if index > 0 {
            super::structure::validate_parent_before(ledger, index)?;
        }
        crate::model::available(&account)
            .map_err(|_error| crate::model::corrupt(account.id))?;

        let direct_child_remaining = super::sums::direct_child_remaining(ledger, index)?;
        if !direct_child_remaining.equals(account.child_delegated_remaining) {
            proof {
                if crate::model::ledger_well_formed(ledger) {
                    assert(crate::invariant::exact_derived_accounting(ledger, index as int));
                    assert(direct_child_remaining.spec_equal(account.child_delegated_remaining));
                }
            }
            return Err(crate::model::corrupt(account.id));
        }
        let direct_operation_reserved =
            super::sums::direct_operation_reserved(ledger, index)?;
        if !direct_operation_reserved.equals(account.operation_reserved) {
            proof {
                if crate::model::ledger_well_formed(ledger) {
                    assert(crate::invariant::exact_derived_accounting(ledger, index as int));
                    assert(direct_operation_reserved.spec_equal(account.operation_reserved));
                }
            }
            return Err(crate::model::corrupt(account.id));
        }
        assert(crate::invariant::exact_derived_accounting(ledger, index as int));

        if account.phase.is_closed() {
            if has_live_work(ledger, account.id) {
                proof {
                    if crate::model::ledger_well_formed(ledger) {
                        assert(crate::invariant::closed_account_has_no_live_work(
                            ledger,
                            index as int,
                        ));
                    }
                }
                return Err(crate::model::corrupt(account.id));
            }
            assert(crate::invariant::closed_account_has_no_live_work(
                ledger,
                index as int,
            ));
        }
        assert(crate::invariant::account_entry_valid(ledger, index as int));
        index += 1;
    }
    assert(crate::model::ledger_conserves(ledger));
    assert(crate::invariant::ledger_account_structure_holds(ledger));
    Ok(())
}

} // verus!
