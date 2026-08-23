//! Executable uniqueness and parent-order checks with exact specification contracts.

use crate::{BudgetError, BudgetLedger};
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

verus! {

fn duplicate_account_before(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    end: usize,
) -> (result: bool)
    ensures
        end <= ledger.accounts@.len() ==> result == exists |prior: int| #![auto]
                0 <= prior < end
                    && crate::identity_model::budget_ids_equal(
                        ledger.accounts[prior].id,
                        budget_id,
                    ),
{
    if end > ledger.accounts.len() {
        return true;
    }
    let mut index = 0;
    while index < end
        invariant
            0 <= index <= end,
            end <= ledger.accounts@.len(),
            forall |prior: int| #![auto]
                0 <= prior < index
                    ==> !crate::identity_model::budget_ids_equal(
                        ledger.accounts[prior].id,
                        budget_id,
                    ),
        decreases end - index,
    {
        if crate::identity_model::budget_id_equal(ledger.accounts[index].id, budget_id) {
            assert(exists |prior: int| #![auto] 0 <= prior < end
                && crate::identity_model::budget_ids_equal(
                    ledger.accounts[prior].id,
                    budget_id,
                ));
            return true;
        }
        index += 1;
    }
    false
}

pub(super) fn validate_account_unique_before(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: Result<(), BudgetError>)
    ensures
        ((index as int) < ledger.accounts@.len()
            && crate::invariant::account_unique_before(ledger, index as int))
            ==> result.is_ok(),
        match result {
            Ok(()) => {
                (index as int) < ledger.accounts@.len()
                    && crate::invariant::account_unique_before(ledger, index as int)
            }
            Err(_) => true,
        },
{
    if index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let account = account_at(ledger, index)?;
    if duplicate_account_before(ledger, account.id, index) {
        return Err(crate::model::corrupt(account.id));
    }
    assert(crate::invariant::account_unique_before(ledger, index as int));
    Ok(())
}

fn duplicate_reservation_before(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
    end: usize,
) -> (result: bool)
    ensures
        end <= ledger.reservations@.len() ==> result == exists |prior: int| #![auto]
                0 <= prior < end
                    && crate::identity_model::reservation_ids_equal(
                        ledger.reservations[prior].request.spec_reservation_id(),
                        reservation_id,
                    ),
{
    if end > ledger.reservations.len() {
        return true;
    }
    let mut index = 0;
    while index < end
        invariant
            0 <= index <= end,
            end <= ledger.reservations@.len(),
            forall |prior: int| #![auto]
                0 <= prior < index
                    ==> !crate::identity_model::reservation_ids_equal(
                        ledger.reservations[prior].request.spec_reservation_id(),
                        reservation_id,
                    ),
        decreases end - index,
    {
        if crate::identity_model::reservation_id_equal(
            ledger.reservations[index].request.verified_reservation_id(),
            reservation_id,
        ) {
            assert(exists |prior: int| #![auto] 0 <= prior < end
                && crate::identity_model::reservation_ids_equal(
                    ledger.reservations[prior].request.spec_reservation_id(),
                    reservation_id,
                ));
            return true;
        }
        index += 1;
    }
    false
}

pub(super) fn validate_reservation_unique_before(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: Result<(), BudgetError>)
    ensures
        ((index as int) < ledger.reservations@.len()
            && crate::invariant::reservation_unique_before(ledger, index as int))
            ==> result.is_ok(),
        match result {
            Ok(()) => {
                (index as int) < ledger.reservations@.len()
                    && crate::invariant::reservation_unique_before(ledger, index as int)
            }
            Err(_) => true,
        },
{
    if index >= ledger.reservations.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let request = ledger.reservations[index].request;
    let reservation_id = request.verified_reservation_id();
    if duplicate_reservation_before(ledger, reservation_id, index) {
        return Err(BudgetError::reservation(
            crate::BudgetErrorKind::CorruptState,
            reservation_id,
        ));
    }
    assert(crate::invariant::reservation_unique_before(ledger, index as int));
    Ok(())
}

pub(super) fn validate_parent_before(
    ledger: &BudgetLedger,
    child_index: usize,
) -> (result: Result<(), BudgetError>)
    ensures
        (crate::invariant::ledger_account_structure_holds(ledger)
            && 0 < child_index < ledger.accounts@.len())
            ==> result.is_ok(),
        match result {
            Ok(()) => {
                0 < child_index < ledger.accounts@.len()
                    && crate::invariant::parent_link_valid(ledger, child_index as int)
            }
            Err(_) => true,
        },
{
    if child_index == 0 || child_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let child = account_at(ledger, child_index)?;
    proof {
        if crate::invariant::ledger_account_structure_holds(ledger) {
            assert(crate::invariant::account_entry_valid(ledger, child_index as int));
            assert(crate::invariant::parent_link_valid(ledger, child_index as int));
            let expected = choose |expected: int| #![auto]
                0 <= expected < child_index
                    && crate::identity_model::parent_matches(
                        ledger.accounts[child_index as int].parent_id,
                        ledger.accounts[expected].id,
                    )
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[child_index as int].revision,
                        ledger.accounts[expected].revision,
                    );
            assert(child.parent_id.is_some());
        }
    }
    if child.parent_id.is_none() {
        return Err(crate::model::corrupt(child.id));
    }
    let mut parent_index = 0;
    while parent_index < child_index
        invariant
            0 <= parent_index <= child_index,
            0 < child_index < ledger.accounts@.len(),
            child.parent_id == ledger.accounts[child_index as int].parent_id,
            child.revision == ledger.accounts[child_index as int].revision,
            crate::invariant::ledger_account_structure_holds(ledger) ==> (
                crate::invariant::parent_link_valid(ledger, child_index as int)
                    && forall |prior: int| #![auto]
                        0 <= prior < parent_index
                            ==> !crate::identity_model::parent_matches(
                                ledger.accounts[child_index as int].parent_id,
                                ledger.accounts[prior].id,
                            )
            ),
        decreases child_index - parent_index,
    {
        if crate::identity_model::parent_matches_id(
            child.parent_id,
            ledger.accounts[parent_index].id,
        ) {
            if !crate::identity_model::revision_equal(
                child.revision,
                ledger.accounts[parent_index].revision,
            ) {
                proof {
                    if crate::invariant::ledger_account_structure_holds(ledger) {
                        let expected = choose |expected: int| #![auto]
                            0 <= expected < child_index
                                && crate::identity_model::parent_matches(
                                    ledger.accounts[child_index as int].parent_id,
                                    ledger.accounts[expected].id,
                                )
                                && crate::identity_model::revisions_equal(
                                    ledger.accounts[child_index as int].revision,
                                    ledger.accounts[expected].revision,
                                );
                        assert(crate::identity_model::budget_ids_equal(
                            ledger.accounts[parent_index as int].id,
                            ledger.accounts[expected].id,
                        ));
                        crate::invariant::matching_accounts_are_unique(
                            ledger,
                            parent_index as int,
                            expected,
                        );
                        assert(parent_index as int == expected);
                    }
                }
                return Err(crate::model::corrupt(child.id));
            }
            assert(crate::identity_model::parent_matches(
                child.parent_id,
                ledger.accounts[parent_index as int].id,
            ));
            assert(child.parent_id == ledger.accounts[child_index as int].parent_id);
            assert(child.revision == ledger.accounts[child_index as int].revision);
            assert(crate::identity_model::revisions_equal(
                ledger.accounts[child_index as int].revision,
                ledger.accounts[parent_index as int].revision,
            ));
            assert(exists |parent: int| #![auto] 0 <= parent < child_index
                && crate::identity_model::parent_matches(
                    ledger.accounts[child_index as int].parent_id,
                    ledger.accounts[parent].id,
                )
                && crate::identity_model::revisions_equal(
                    ledger.accounts[child_index as int].revision,
                    ledger.accounts[parent].revision,
                ));
            return Ok(());
        }
        proof {
            assert(!crate::identity_model::parent_matches(
                ledger.accounts[child_index as int].parent_id,
                ledger.accounts[parent_index as int].id,
            ));
        }
        parent_index += 1;
    }
    proof {
        if crate::invariant::ledger_account_structure_holds(ledger) {
            assert(crate::invariant::parent_link_valid(ledger, child_index as int));
            let expected = choose |expected: int| #![auto]
                0 <= expected < child_index
                    && crate::identity_model::parent_matches(
                        ledger.accounts[child_index as int].parent_id,
                        ledger.accounts[expected].id,
                    )
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[child_index as int].revision,
                        ledger.accounts[expected].revision,
                    );
            assert(0 <= expected < parent_index);
            assert(!crate::identity_model::parent_matches(
                ledger.accounts[child_index as int].parent_id,
                ledger.accounts[expected].id,
            ));
        }
    }
    Err(crate::model::corrupt(child.id))
}

fn account_at(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: Result<crate::state::BudgetAccount, BudgetError>)
    ensures
        match result {
            Ok(account) => {
                (index as int) < ledger.accounts@.len()
                    && account.id == ledger.accounts[index as int].id
                    && account.parent_id == ledger.accounts[index as int].parent_id
                    && account.revision == ledger.accounts[index as int].revision
            }
            Err(_) => (index as int) >= ledger.accounts@.len(),
        },
{
    if index >= ledger.accounts.len() {
        Err(crate::model::corrupt(ledger.root_id))
    } else {
        Ok(ledger.accounts[index])
    }
}

} // verus!
