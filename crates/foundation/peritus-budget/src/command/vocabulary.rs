//! Closed command vocabulary for the budget reducer.

// Cargo Verus lowers documented payload variants into undocumented synthetic methods. The pinned
// toolchain does not propagate item- or variant-level lint attributes to those methods, so the
// exception must surround its synthetic artifact. This module deliberately contains only the one
// fully documented closed vocabulary.
#![allow(missing_docs)]

use super::{
    Activation, AmbiguousFinalization, BudgetRequest, ChildBudgetRequest, ReservationReference,
    UsageObservation,
};
use peritus_types::BudgetId;

#[doc = " Complete closed command family accepted by [`crate::BudgetLedger::transition`]."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetCommand {
    #[doc = " Allocate an immutable child account."]
    AllocateChild(ChildBudgetRequest),
    #[doc = " Charge known use and reserve an operation ceiling atomically."]
    Begin(BudgetRequest),
    #[doc = " Mark a held operation active."]
    Activate(Activation),
    #[doc = " Reconcile a cumulative usage high-water observation."]
    ObserveUsage(UsageObservation),
    #[doc = " Consume the complete outstanding ceiling from exact settlement evidence."]
    SettleExact(ReservationReference),
    #[doc = " Logically release a held reservation using a correlated non-activation claim."]
    #[doc = ""]
    #[doc = " This is not effect or commit authority. `REF-C0-B1-COMMIT-ONCE` requires C0 to establish"]
    #[doc = " the external negative fact from its own authoritative target or journal observation."]
    CancelHeld(ReservationReference),
    #[doc = " Conservatively consume the outstanding ceiling after ambiguity."]
    FinalizeAmbiguous(AmbiguousFinalization),
    #[doc = " Stop an account and its descendants from beginning or allocating new work."]
    Seal(BudgetId),
    #[doc = " Close a quiescent account, releasing only unused child delegation."]
    Close(BudgetId),
}
