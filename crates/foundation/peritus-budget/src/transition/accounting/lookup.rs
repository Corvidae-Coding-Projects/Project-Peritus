//! Exact account/reservation lookup and live-work validation.

#[cfg(verus_only)]
use crate::BudgetAccountPhase;
use crate::{BudgetError, BudgetErrorKind, BudgetLedger, ReservationReference};
use peritus_types::{ActionId, BudgetId, BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn has_live_work(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> (result: bool)
    ensures
        result ==> !crate::invariant::budget_has_no_live_work(ledger, budget_id),
        !result ==> crate::invariant::budget_has_no_live_work(ledger, budget_id),
{
    let mut reservation_index = 0;
    while reservation_index < ledger.reservations.len()
        invariant
            0 <= reservation_index <= ledger.reservations.len(),
            forall |checked: int| #![auto]
                0 <= checked < reservation_index
                    && crate::identity_model::budget_ids_equal(
                        ledger.reservations[checked].request.spec_budget_id(),
                        budget_id,
                    )
                    ==> !ledger.reservations[checked].phase.spec_is_live(),
        decreases ledger.reservations.len() - reservation_index,
    {
        let record = &ledger.reservations[reservation_index];
        let matches_budget =
            crate::identity_model::budget_id_equal(record.request.budget_id(), budget_id);
        let live = record.phase.is_live();
        if matches_budget && live {
            return true;
        }
        assert(matches_budget ==> !live);
        reservation_index += 1;
    }
    let mut account_index = 0;
    while account_index < ledger.accounts.len()
        invariant
            0 <= account_index <= ledger.accounts.len(),
            forall |checked: int| #![auto]
                0 <= checked < ledger.reservations@.len()
                    && crate::identity_model::budget_ids_equal(
                        ledger.reservations[checked].request.spec_budget_id(),
                        budget_id,
                    )
                    ==> !ledger.reservations[checked].phase.spec_is_live(),
            forall |checked: int| #![auto]
                0 <= checked < account_index
                    && crate::identity_model::parent_matches(
                        ledger.accounts[checked].parent_id,
                        budget_id,
                    )
                    ==> ledger.accounts[checked].phase == BudgetAccountPhase::Closed,
        decreases ledger.accounts.len() - account_index,
    {
        let account = &ledger.accounts[account_index];
        let matches_parent =
            crate::identity_model::parent_matches_id(account.parent_id, budget_id);
        let closed = account.phase.is_closed();
        if matches_parent && !closed {
            return true;
        }
        assert(matches_parent ==> closed);
        assert(matches_parent ==> account.phase.spec_is_closed());
        account_index += 1;
    }
    assert(crate::invariant::budget_has_no_live_work(ledger, budget_id));
    false
}

pub(in crate::transition) fn require_binding(
    record: &crate::state::ReservationRecord,
    action_id: ActionId,
    action_digest: Sha256Digest,
) -> (result: Result<(), BudgetError>)
    ensures
        match result {
            Ok(()) => crate::identity_model::action_ids_equal(
                record.request.spec_action_id(),
                action_id,
            ) && crate::identity_model::digests_equal(
                record.request.spec_action_digest(),
                action_digest,
            ),
            Err(error) => {
                crate::reachability::exact_reservation_error(
                        error,
                        BudgetErrorKind::BindingMismatch,
                        record.request.spec_reservation_id(),
                    )
                    && (!crate::identity_model::action_ids_equal(
                        record.request.spec_action_id(),
                        action_id,
                    ) || !crate::identity_model::digests_equal(
                        record.request.spec_action_digest(),
                        action_digest,
                    ))
            }
        },
{
    if !crate::identity_model::action_id_equal(
        record.request.verified_action_id(),
        action_id,
    ) || !crate::identity_model::digest_equal(
        record.request.verified_action_digest(),
        action_digest,
    ) {
        return Err(BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            record.request.verified_reservation_id(),
        ));
    }
    Ok(())
}

pub(in crate::transition) fn require_reference_binding(
    record: &crate::state::ReservationRecord,
    reference: ReservationReference,
) -> (result: Result<(), BudgetError>)
    ensures
        match result {
            Ok(()) => crate::reachability::reference_binding_guard(*record, reference),
            Err(error) => {
                crate::reachability::exact_reservation_error(
                        error,
                        BudgetErrorKind::BindingMismatch,
                        record.request.spec_reservation_id(),
                    )
                    && !crate::reachability::reference_binding_guard(*record, reference)
            }
        },
{
    require_binding(
        record,
        reference.verified_action_id(),
        reference.verified_action_digest(),
    )
}

pub(in crate::transition) fn require_account(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<usize, BudgetError>)
    ensures
        match result {
            Ok(index) => {
                (index as int) < ledger.accounts@.len()
                    && crate::identity_model::budget_ids_equal(
                        ledger.accounts[index as int].id,
                        budget_id,
                    )
            }
            Err(error) => {
                crate::reachability::exact_budget_error(
                        error,
                        BudgetErrorKind::UnknownBudget,
                        budget_id,
                    )
                    && (forall |index: int| #![auto]
                        0 <= index < ledger.accounts@.len()
                            ==> !crate::identity_model::budget_ids_equal(
                                ledger.accounts[index].id,
                                budget_id,
                            ))
            }
        },
{
    match find_account(ledger, budget_id) {
        Some(index) => Ok(index),
        None => Err(BudgetError::budget(BudgetErrorKind::UnknownBudget, budget_id)),
    }
}

pub(in crate::transition) fn require_reservation(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> (result: Result<usize, BudgetError>)
    ensures
        match result {
            Ok(index) => {
                (index as int) < ledger.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        ledger.reservations[index as int].request.spec_reservation_id(),
                        reservation_id,
                    )
            }
            Err(error) => {
                crate::reachability::exact_reservation_error(
                        error,
                        BudgetErrorKind::UnknownReservation,
                        reservation_id,
                    )
                    && (forall |index: int| #![auto]
                        0 <= index < ledger.reservations@.len()
                            ==> !crate::identity_model::reservation_ids_equal(
                                ledger.reservations[index].request.spec_reservation_id(),
                                reservation_id,
                            ))
            }
        },
{
    match find_reservation(ledger, reservation_id) {
        Some(index) => Ok(index),
        None => Err(BudgetError::reservation(
            BudgetErrorKind::UnknownReservation,
            reservation_id,
        )),
    }
}

pub(in crate::transition) fn find_account(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> (result: Option<usize>)
    ensures
        match result {
            Some(index) => {
                (index as int) < ledger.accounts@.len()
                    && crate::identity_model::budget_ids_equal(
                        ledger.accounts[index as int].id,
                        budget_id,
                    )
            }
            None => forall |index: int| #![auto]
                0 <= index < ledger.accounts@.len()
                    ==> !crate::identity_model::budget_ids_equal(
                        ledger.accounts[index].id,
                        budget_id,
                    ),
        },
{
    let mut index = 0;
    while index < ledger.accounts.len()
        invariant
            0 <= index <= ledger.accounts.len(),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> !crate::identity_model::budget_ids_equal(
                        ledger.accounts[checked].id,
                        budget_id,
                    ),
        decreases ledger.accounts.len() - index,
    {
        if crate::identity_model::budget_id_equal(ledger.accounts[index].id, budget_id) {
            return Some(index);
        }
        index += 1;
    }
    None
}

pub(in crate::transition) fn find_reservation(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> (result: Option<usize>)
    ensures
        match result {
            Some(index) => {
                (index as int) < ledger.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        ledger.reservations[index as int].request.spec_reservation_id(),
                        reservation_id,
                    )
            }
            None => forall |index: int| #![auto]
                0 <= index < ledger.reservations@.len()
                    ==> !crate::identity_model::reservation_ids_equal(
                        ledger.reservations[index].request.spec_reservation_id(),
                        reservation_id,
                    ),
        },
{
    let mut index = 0;
    while index < ledger.reservations.len()
        invariant
            0 <= index <= ledger.reservations.len(),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> !crate::identity_model::reservation_ids_equal(
                        ledger.reservations[checked].request.spec_reservation_id(),
                        reservation_id,
                    ),
        decreases ledger.reservations.len() - index,
    {
        if crate::identity_model::reservation_id_equal(
            ledger.reservations[index].request.verified_reservation_id(),
            reservation_id,
        ) {
            return Some(index);
        }
        index += 1;
    }
    None
}

} // verus!
