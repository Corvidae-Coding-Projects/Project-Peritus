//! Verified hierarchical resource-budget accounting for Peritus.
//!
//! The public reducer is value-in/value-out: a rejected command leaves the caller's ledger
//! untouched, while an accepted command returns a new ledger and a non-authorizing receipt.

mod accounting_model;
mod accounting_prefix_model;
mod amounts;
mod command;
mod failure;
mod identity_model;
mod limits;
mod model;
mod snapshot;
mod state;
mod transition;
pub use amounts::BudgetAmounts;
pub use command::{
    Activation, AmbiguousFinalization, BudgetCommand, BudgetRequest, ChildBudgetRequest,
    ReservationReference, UsageFinality, UsageObservation,
};
pub use failure::{
    AmountArithmeticError, ArithmeticKind, BudgetError, BudgetErrorKind, BudgetRecovery,
};
pub use limits::{BudgetDimension, BudgetDimensionSet, BudgetLimits};
pub use snapshot::{BudgetSnapshot, ReservationSnapshot};
pub use state::{
    BudgetAccountPhase, BudgetLedger, BudgetOperation, BudgetReceipt, BudgetReceiptKind,
    BudgetTransition, ReservationPhase,
};
