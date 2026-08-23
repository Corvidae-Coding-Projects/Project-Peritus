//! Executable monotonicity, immutable-prefix, and ancestor-propagation checks.

mod propagation;

use self::propagation::validate_ancestor_propagation;

use crate::{BudgetError, BudgetErrorKind, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(super) fn validate(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> (result: Result<(), BudgetError>)
    ensures
        (crate::model::ledger_well_formed(before)
            && crate::reachability::complete_refinement(before, after)) ==> result.is_ok(),
        match result {
            Ok(()) => {
                crate::model::ledger_consumption_monotonic(before, after)
                    && crate::model::ledger_high_water_monotonic(before, after)
                    && crate::refinement_model::ledger_identity_stable(before, after)
                    && crate::refinement_model::ancestor_consumption_propagates(before, after)
            }
            Err(_) => true,
        },
{
    if !crate::identity_model::budget_id_equal(before.root_id, after.root_id) {
        return Err(crate::model::corrupt(after.root_id));
    }
    validate_accounts(before, after)?;
    validate_reservations(before, after)?;
    validate_ancestor_propagation(before, after)?;
    assert(crate::refinement_model::ledger_identity_stable(before, after));
    Ok(())
}

fn validate_accounts(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> (result: Result<(), BudgetError>)
    ensures
        (crate::model::ledger_consumption_monotonic(before, after)
            && crate::refinement_model::ledger_identity_stable(before, after))
            ==> result.is_ok(),
        match result {
            Ok(()) => {
                crate::model::ledger_consumption_monotonic(before, after)
                    && before.accounts@.len() <= after.accounts@.len()
                    && forall |index: int| #![auto]
                        0 <= index < before.accounts@.len()
                            ==> crate::refinement_model::account_identity_stable(
                                before, after, index,
                            )
            }
            Err(_) => true,
        },
{
    if before.accounts.len() > after.accounts.len() {
        return Err(crate::model::corrupt(after.root_id));
    }
    let mut index = 0;
    while index < before.accounts.len()
        invariant
            0 <= index <= before.accounts.len(),
            before.accounts@.len() <= after.accounts@.len(),
            (crate::model::ledger_consumption_monotonic(before, after)
                && crate::refinement_model::ledger_identity_stable(before, after)) ==> (
                before.accounts@.len() <= after.accounts@.len()
                    && forall |checked: int| #![auto]
                        0 <= checked < before.accounts@.len()
                            ==> crate::model::consumption_monotonic(
                                before.accounts[checked],
                                after.accounts[checked],
                            )
                                && crate::refinement_model::account_identity_stable(
                                    before,
                                    after,
                                    checked,
                                )
            ),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::model::consumption_monotonic(
                        before.accounts[checked],
                        after.accounts[checked],
                    ),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::refinement_model::account_identity_stable(
                        before, after, checked,
                    ),
        decreases before.accounts.len() - index,
    {
        let prior = account_at(before, index)?;
        let next = account_at(after, index)?;
        proof {
            if crate::model::ledger_consumption_monotonic(before, after)
                && crate::refinement_model::ledger_identity_stable(before, after)
            {
                assert(crate::model::consumption_monotonic(
                    before.accounts[index as int],
                    after.accounts[index as int],
                ));
                assert(crate::refinement_model::account_identity_stable(
                    before,
                    after,
                    index as int,
                ));
            }
        }
        let same_id = crate::identity_model::budget_id_equal(prior.id, next.id);
        let same_parent = crate::identity_model::parent_equal(prior.parent_id, next.parent_id);
        let same_revision = crate::identity_model::revision_equal(prior.revision, next.revision);
        let same_limits = prior.limits.amounts().equals(next.limits.amounts());
        let monotonic = prior.consumed.fits_within(next.consumed);
        if !same_id || !same_parent || !same_revision || !same_limits || !monotonic {
            return Err(crate::model::corrupt(prior.id));
        }
        assert(crate::model::consumption_monotonic(
            before.accounts[index as int],
            after.accounts[index as int],
        ));
        assert(crate::refinement_model::account_identity_stable(
            before,
            after,
            index as int,
        ));
        index += 1;
    }
    assert(crate::model::ledger_consumption_monotonic(before, after));
    Ok(())
}

fn validate_reservations(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> (result: Result<(), BudgetError>)
    ensures
        (crate::model::ledger_high_water_monotonic(before, after)
            && crate::refinement_model::ledger_identity_stable(before, after))
            ==> result.is_ok(),
        match result {
            Ok(()) => {
                crate::model::ledger_high_water_monotonic(before, after)
                    && before.reservations@.len() <= after.reservations@.len()
                    && forall |index: int| #![auto]
                        0 <= index < before.reservations@.len()
                            ==> crate::refinement_model::reservation_identity_stable(
                                before, after, index,
                            )
            }
            Err(_) => true,
        },
{
    if before.reservations.len() > after.reservations.len() {
        return Err(crate::model::corrupt(after.root_id));
    }
    let mut index = 0;
    while index < before.reservations.len()
        invariant
            0 <= index <= before.reservations.len(),
            before.reservations@.len() <= after.reservations@.len(),
            (crate::model::ledger_high_water_monotonic(before, after)
                && crate::refinement_model::ledger_identity_stable(before, after)) ==> (
                before.reservations@.len() <= after.reservations@.len()
                    && forall |checked: int| #![auto]
                        0 <= checked < before.reservations@.len()
                            ==> crate::model::amounts_le(
                                before.reservations[checked].observed,
                                after.reservations[checked].observed,
                            )
                                && crate::refinement_model::reservation_identity_stable(
                                    before,
                                    after,
                                    checked,
                                )
            ),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::model::amounts_le(
                        before.reservations[checked].observed,
                        after.reservations[checked].observed,
                    ),
            forall |checked: int| #![auto]
                0 <= checked < index
                    ==> crate::refinement_model::reservation_identity_stable(
                        before, after, checked,
                    ),
        decreases before.reservations.len() - index,
    {
        let prior = reservation_at(before, index)?;
        let next = reservation_at(after, index)?;
        proof {
            if crate::model::ledger_high_water_monotonic(before, after)
                && crate::refinement_model::ledger_identity_stable(before, after)
            {
                assert(crate::model::amounts_le(
                    before.reservations[index as int].observed,
                    after.reservations[index as int].observed,
                ));
                assert(crate::refinement_model::reservation_identity_stable(
                    before,
                    after,
                    index as int,
                ));
            }
        }
        if !request_equal(prior.request, next.request)
            || !prior.observed.fits_within(next.observed)
        {
            return Err(BudgetError::reservation(
                BudgetErrorKind::CorruptState,
                prior.request.reservation_id(),
            ));
        }
        assert(crate::refinement_model::reservation_identity_stable(
            before,
            after,
            index as int,
        ));
        index += 1;
    }
    assert(crate::model::ledger_high_water_monotonic(before, after));
    Ok(())
}

const fn request_equal(
    left: crate::BudgetRequest,
    right: crate::BudgetRequest,
) -> (result: bool)
    ensures result == crate::refinement_model::requests_equal(left, right),
{
    crate::identity_model::reservation_id_equal(
        left.verified_reservation_id(),
        right.verified_reservation_id(),
    ) && crate::identity_model::budget_id_equal(left.budget_id(), right.budget_id())
        && crate::identity_model::revision_equal(
            left.verified_revision(),
            right.verified_revision(),
        )
        && crate::identity_model::action_id_equal(
            left.verified_action_id(),
            right.verified_action_id(),
        )
        && crate::identity_model::digest_equal(
            left.verified_action_digest(),
            right.verified_action_digest(),
        )
        && left.verified_consume_now().equals(right.verified_consume_now())
        && left.reserve().equals(right.reserve())
}

fn account_at(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: Result<crate::state::BudgetAccount, BudgetError>)
    ensures
        match result {
            Ok(account) => {
                (index as int) < ledger.accounts@.len()
                    && account == ledger.accounts[index as int]
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

fn reservation_at(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: Result<crate::state::ReservationRecord, BudgetError>)
    ensures
        match result {
            Ok(record) => {
                (index as int) < ledger.reservations@.len()
                    && record == ledger.reservations[index as int]
            }
            Err(_) => (index as int) >= ledger.reservations@.len(),
        },
{
    if index >= ledger.reservations.len() {
        Err(crate::model::corrupt(ledger.root_id))
    } else {
        Ok(ledger.reservations[index])
    }
}

} // verus!
