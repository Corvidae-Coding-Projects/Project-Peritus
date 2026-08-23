//! Checked ledger queries and replay validation contracts.

use super::BudgetLedger;
use crate::{BudgetError, BudgetSnapshot, ReservationSnapshot};
#[cfg(verus_only)]
use crate::BudgetAmounts;
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

verus! {

impl BudgetLedger {
    /// Opaque total refinement relation for a checked account query.
    pub closed spec fn spec_account_result(
        &self,
        budget_id: BudgetId,
        result: Result<BudgetSnapshot, BudgetError>,
    ) -> bool {
        if !crate::model::ledger_well_formed(self) {
            match result {
                Err(error) => crate::reachability::exact_corrupt_root(self, error),
                Ok(_) => false,
            }
        } else {
            match result {
                Ok(snapshot) => exists |index: int, available: BudgetAmounts| #![auto]
                    0 <= index < self.accounts@.len()
                        && crate::identity_model::budget_ids_equal(
                            self.accounts[index].id, budget_id,
                        )
                        && crate::model::available_is_exact(
                            self.accounts[index], available,
                        )
                        && crate::snapshot::account_snapshot_exact(
                            self.accounts[index], snapshot, available,
                        ),
                Err(error) => {
                    (forall |index: int| #![auto]
                        0 <= index < self.accounts@.len()
                            ==> !crate::identity_model::budget_ids_equal(
                                self.accounts[index].id, budget_id,
                            ))
                        && crate::reachability::exact_budget_error(
                            error, crate::BudgetErrorKind::UnknownBudget, budget_id,
                        )
                }
            }
        }
    }

    pub(crate) proof fn account_corrupt_result(
        &self,
        budget_id: BudgetId,
        error: BudgetError,
    )
        requires
            !crate::model::ledger_well_formed(self),
            crate::reachability::exact_corrupt_root(self, error),
        ensures self.spec_account_result(budget_id, Err(error)),
    {
        reveal(BudgetLedger::spec_account_result);
    }

    pub(crate) proof fn account_unknown_result(
        &self,
        budget_id: BudgetId,
        error: BudgetError,
    )
        requires
            crate::model::ledger_well_formed(self),
            forall |index: int| #![auto]
                0 <= index < self.accounts@.len()
                    ==> !crate::identity_model::budget_ids_equal(
                        self.accounts[index].id, budget_id,
                    ),
            crate::reachability::exact_budget_error(
                error, crate::BudgetErrorKind::UnknownBudget, budget_id,
            ),
        ensures self.spec_account_result(budget_id, Err(error)),
    {
        reveal(BudgetLedger::spec_account_result);
    }

    pub(crate) proof fn account_snapshot_result(
        &self,
        budget_id: BudgetId,
        index: int,
        available: BudgetAmounts,
        snapshot: BudgetSnapshot,
    )
        requires
            crate::model::ledger_well_formed(self),
            0 <= index < self.accounts@.len(),
            crate::identity_model::budget_ids_equal(self.accounts[index].id, budget_id),
            crate::model::available_is_exact(self.accounts[index], available),
            crate::snapshot::account_snapshot_exact(
                self.accounts[index], snapshot, available,
            ),
        ensures self.spec_account_result(budget_id, Ok(snapshot)),
    {
        reveal(BudgetLedger::spec_account_result);
        assert(exists |candidate: int, exact_available: BudgetAmounts| #![auto]
            candidate == index
                && exact_available == available
                && 0 <= candidate < self.accounts@.len()
                && crate::identity_model::budget_ids_equal(
                    self.accounts[candidate].id, budget_id,
                )
                && crate::model::available_is_exact(
                    self.accounts[candidate], exact_available,
                )
                && crate::snapshot::account_snapshot_exact(
                    self.accounts[candidate], snapshot, exact_available,
                ));
    }

    /// Returns a checked immutable view of one account.
    ///
    /// # Errors
    ///
    /// Returns [`crate::BudgetErrorKind::UnknownBudget`] when `budget_id` is absent, or a
    /// corruption/arithmetic error when stored accounting cannot produce an exact available value.
    pub fn account(
        &self,
        budget_id: BudgetId,
    ) -> (result: Result<BudgetSnapshot, BudgetError>)
        ensures self.spec_account_result(budget_id, result),
    {
        crate::transition::snapshot_account(self, budget_id)
    }

    /// Opaque total refinement relation for a checked reservation query.
    pub closed spec fn spec_reservation_result(
        &self,
        reservation_id: BudgetReservationId,
        result: Result<ReservationSnapshot, BudgetError>,
    ) -> bool {
        if !crate::model::ledger_well_formed(self) {
            match result {
                Err(error) => crate::reachability::exact_corrupt_root(self, error),
                Ok(_) => false,
            }
        } else {
            match result {
                Ok(snapshot) => exists |index: int, outstanding: BudgetAmounts| #![auto]
                    0 <= index < self.reservations@.len()
                        && crate::identity_model::reservation_ids_equal(
                            self.reservations[index].request.spec_reservation_id(),
                            reservation_id,
                        )
                        && crate::snapshot::outstanding_is_exact(
                            self.reservations[index], outstanding,
                        )
                        && crate::snapshot::reservation_snapshot_exact(
                            self.reservations[index], snapshot, outstanding,
                        ),
                Err(error) => {
                    (forall |index: int| #![auto]
                        0 <= index < self.reservations@.len()
                            ==> !crate::identity_model::reservation_ids_equal(
                                self.reservations[index].request.spec_reservation_id(),
                                reservation_id,
                            ))
                        && crate::reachability::exact_reservation_error(
                            error, crate::BudgetErrorKind::UnknownReservation, reservation_id,
                        )
                }
            }
        }
    }

    pub(crate) proof fn reservation_corrupt_result(
        &self,
        reservation_id: BudgetReservationId,
        error: BudgetError,
    )
        requires
            !crate::model::ledger_well_formed(self),
            crate::reachability::exact_corrupt_root(self, error),
        ensures self.spec_reservation_result(reservation_id, Err(error)),
    {
        reveal(BudgetLedger::spec_reservation_result);
    }

    pub(crate) proof fn reservation_unknown_result(
        &self,
        reservation_id: BudgetReservationId,
        error: BudgetError,
    )
        requires
            crate::model::ledger_well_formed(self),
            forall |index: int| #![auto]
                0 <= index < self.reservations@.len()
                    ==> !crate::identity_model::reservation_ids_equal(
                        self.reservations[index].request.spec_reservation_id(),
                        reservation_id,
                    ),
            crate::reachability::exact_reservation_error(
                error, crate::BudgetErrorKind::UnknownReservation, reservation_id,
            ),
        ensures self.spec_reservation_result(reservation_id, Err(error)),
    {
        reveal(BudgetLedger::spec_reservation_result);
    }

    pub(crate) proof fn reservation_snapshot_result(
        &self,
        reservation_id: BudgetReservationId,
        index: int,
        outstanding: BudgetAmounts,
        snapshot: ReservationSnapshot,
    )
        requires
            crate::model::ledger_well_formed(self),
            0 <= index < self.reservations@.len(),
            crate::identity_model::reservation_ids_equal(
                self.reservations[index].request.spec_reservation_id(), reservation_id,
            ),
            crate::snapshot::outstanding_is_exact(
                self.reservations[index], outstanding,
            ),
            crate::snapshot::reservation_snapshot_exact(
                self.reservations[index], snapshot, outstanding,
            ),
        ensures self.spec_reservation_result(reservation_id, Ok(snapshot)),
    {
        reveal(BudgetLedger::spec_reservation_result);
        assert(exists |candidate: int, exact_outstanding: BudgetAmounts| #![auto]
            candidate == index
                && exact_outstanding == outstanding
                && 0 <= candidate < self.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    self.reservations[candidate].request.spec_reservation_id(),
                    reservation_id,
                )
                && crate::snapshot::outstanding_is_exact(
                    self.reservations[candidate], exact_outstanding,
                )
                && crate::snapshot::reservation_snapshot_exact(
                    self.reservations[candidate], snapshot, exact_outstanding,
                ));
    }

    /// Returns an immutable view of one reservation tombstone.
    ///
    /// # Errors
    ///
    /// Returns [`crate::BudgetErrorKind::UnknownReservation`] when `reservation_id` is absent, or
    /// a corruption/arithmetic error if its outstanding amount cannot be represented exactly.
    pub fn reservation(
        &self,
        reservation_id: BudgetReservationId,
    ) -> (result: Result<ReservationSnapshot, BudgetError>)
        ensures self.spec_reservation_result(reservation_id, result),
    {
        crate::transition::snapshot_reservation(self, reservation_id)
    }

    /// Opaque total refinement relation for checked replay validation.
    pub closed spec fn spec_validation_result(
        &self,
        result: Result<(), BudgetError>,
    ) -> bool {
        match result {
            Ok(()) => crate::model::ledger_well_formed(self),
            Err(error) => {
                !crate::model::ledger_well_formed(self)
                    && crate::reachability::exact_corrupt_root(self, error)
            }
        }
    }

    pub(crate) proof fn valid_validation_result(&self)
        requires crate::model::ledger_well_formed(self),
        ensures self.spec_validation_result(Ok(())),
    {
        reveal(BudgetLedger::spec_validation_result);
    }

    pub(crate) proof fn corrupt_validation_result(&self, error: BudgetError)
        requires
            !crate::model::ledger_well_formed(self),
            crate::reachability::exact_corrupt_root(self, error),
        ensures self.spec_validation_result(Err(error)),
    {
        reveal(BudgetLedger::spec_validation_result);
    }

    /// Validates tree shape and all conservation equations.
    ///
    /// This is suitable for checked replay boundaries. Normal transitions also validate their
    /// input and output state and never partially mutate the caller's value.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption error when any stored relation is invalid.
    pub fn validate(&self) -> (result: Result<(), BudgetError>)
        ensures self.spec_validation_result(result),
    {
        crate::transition::validate(self)
    }
}

} // verus!
