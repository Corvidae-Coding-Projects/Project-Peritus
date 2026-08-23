//! Complete executable reservation-tombstone and retry-history validation.

use super::super::accounting::find_account;
use crate::{BudgetError, BudgetErrorKind, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(super) fn validate(ledger: &BudgetLedger) -> (result: Result<(), BudgetError>)
    ensures
        crate::model::ledger_well_formed(ledger) ==> result.is_ok(),
        match result {
            Ok(()) => crate::invariant::ledger_reservation_structure_holds(ledger),
            Err(_) => true,
        },
{
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::invariant::ledger_structure_holds(ledger));
            assert(crate::invariant::ledger_account_structure_holds(ledger));
            assert(crate::invariant::ledger_reservation_structure_holds(ledger));
        }
    }
    let mut index = 0;
    while index < ledger.reservations.len()
        invariant
            0 <= index <= ledger.reservations.len(),
            crate::model::ledger_well_formed(ledger) ==> (
                crate::invariant::ledger_account_structure_holds(ledger)
                    && crate::invariant::ledger_reservation_structure_holds(ledger)
            ),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::invariant::reservation_entry_valid(ledger, checked),
        decreases ledger.reservations.len() - index,
    {
        let record = ledger.reservations[index];
        let reservation_id = record.request.verified_reservation_id();
        proof {
            if crate::model::ledger_well_formed(ledger) {
                assert(crate::invariant::reservation_entry_valid(ledger, index as int));
            }
        }
        super::structure::validate_reservation_unique_before(ledger, index)?;
        let account_index = find_account(ledger, record.request.budget_id()).ok_or_else(|| {
            BudgetError::reservation(BudgetErrorKind::CorruptState, reservation_id)
        })?;
        proof {
            if crate::model::ledger_well_formed(ledger) {
                let expected = choose |expected: int| #![auto]
                    0 <= expected < ledger.accounts@.len()
                        && crate::identity_model::budget_ids_equal(
                            ledger.reservations[index as int].request.spec_budget_id(),
                            ledger.accounts[expected].id,
                        )
                        && crate::identity_model::revisions_equal(
                            ledger.reservations[index as int].request.spec_revision(),
                            ledger.accounts[expected].revision,
                        );
                crate::invariant::matching_accounts_are_unique(
                    ledger,
                    account_index as int,
                    expected,
                );
                assert(account_index as int == expected);
            }
        }
        if !crate::identity_model::revision_equal(
            record.request.verified_revision(),
            ledger.accounts[account_index].revision,
        ) || !record.observed.fits_within(record.request.reserve()) {
            proof {
                if crate::model::ledger_well_formed(ledger) {
                    assert(crate::invariant::reservation_entry_valid(ledger, index as int));
                }
            }
            return Err(BudgetError::reservation(
                BudgetErrorKind::CorruptState,
                reservation_id,
            ));
        }
        super::records::validate_record_phase(&record)?;
        let retry = match super::super::lifecycle::retry_required(ledger, record.request, index) {
            Ok(retry) => retry,
            Err(_error) => {
                proof {
                    if crate::model::ledger_well_formed(ledger) {
                        assert(crate::invariant::attempt_history_valid(
                            ledger,
                            index as int,
                        ));
                    }
                }
                return Err(BudgetError::reservation(
                    BudgetErrorKind::CorruptState,
                    reservation_id,
                ));
            }
        };
        if super::super::lifecycle::validate_attempt_charge(record.request, retry).is_err() {
            proof {
                if crate::model::ledger_well_formed(ledger) {
                    assert(crate::invariant::attempt_history_valid(
                        ledger,
                        index as int,
                    ));
                }
            }
            return Err(BudgetError::reservation(
                BudgetErrorKind::CorruptState,
                reservation_id,
            ));
        }
        assert(crate::invariant::attempt_history_valid(ledger, index as int));
        assert(crate::invariant::reservation_entry_valid(ledger, index as int));
        index += 1;
    }
    assert(crate::invariant::ledger_reservation_structure_holds(ledger));
    Ok(())
}

} // verus!
