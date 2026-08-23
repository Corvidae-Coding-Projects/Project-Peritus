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
use vstd::prelude::*;

verus! {

/// Complete closed command family accepted by [`crate::BudgetLedger::transition`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetCommand {
    /// Allocate an immutable child account.
    AllocateChild(ChildBudgetRequest),
    /// Charge known use and reserve an operation ceiling atomically.
    Begin(BudgetRequest),
    /// Mark a held operation active.
    Activate(Activation),
    /// Reconcile a cumulative usage high-water observation.
    ObserveUsage(UsageObservation),
    /// Consume the complete outstanding ceiling from exact settlement evidence.
    SettleExact(ReservationReference),
    /// Logically release a held reservation using a correlated non-activation claim.
    ///
    /// This is not effect or commit authority. `REF-C0-B1-COMMIT-ONCE` requires C0 to establish
    /// the external negative fact from its own authoritative target or journal observation.
    CancelHeld(ReservationReference),
    /// Conservatively consume the outstanding ceiling after ambiguity.
    FinalizeAmbiguous(AmbiguousFinalization),
    /// Stop an account and its descendants from beginning or allocating new work.
    Seal(BudgetId),
    /// Close a quiescent account, releasing only unused child delegation.
    Close(BudgetId),
}

} // verus!
