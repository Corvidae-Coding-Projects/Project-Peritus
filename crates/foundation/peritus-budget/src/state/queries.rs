//! Checked ledger queries and replay validation contracts.

use super::BudgetLedger;
use crate::{BudgetError, BudgetSnapshot, ReservationSnapshot};

use peritus_types::{BudgetId, BudgetReservationId};

impl BudgetLedger {
    #[doc = " Returns a checked immutable view of one account."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`crate::BudgetErrorKind::UnknownBudget`] when `budget_id` is absent, or a"]
    #[doc = " corruption/arithmetic error when stored accounting cannot produce an exact available value."]
    pub fn account(&self, budget_id: BudgetId) -> Result<BudgetSnapshot, BudgetError> {
        crate::transition::snapshot_account(self, budget_id)
    }
    #[doc = " Returns an immutable view of one reservation tombstone."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`crate::BudgetErrorKind::UnknownReservation`] when `reservation_id` is absent, or"]
    #[doc = " a corruption/arithmetic error if its outstanding amount cannot be represented exactly."]
    pub fn reservation(
        &self,
        reservation_id: BudgetReservationId,
    ) -> Result<ReservationSnapshot, BudgetError> {
        crate::transition::snapshot_reservation(self, reservation_id)
    }
    #[doc = " Validates tree shape and all conservation equations."]
    #[doc = ""]
    #[doc = " This is suitable for checked replay boundaries. Normal transitions also validate their"]
    #[doc = " input and output state and never partially mutate the caller\'s value."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed corruption error when any stored relation is invalid."]
    pub fn validate(&self) -> Result<(), BudgetError> {
        crate::transition::validate(self)
    }
}
