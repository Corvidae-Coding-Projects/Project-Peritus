//! Closed command dispatch into command-local exact reducers.

use crate::{BudgetCommand, BudgetError, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

verus! {

pub(super) fn apply_validated(
    ledger: &mut BudgetLedger,
    command: BudgetCommand,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                command,
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                command,
                error,
            ),
        },
{
    match command {
        BudgetCommand::AllocateChild(request) => super::allocation::allocate_child(ledger, request),
        BudgetCommand::Begin(request) => super::allocation::begin(ledger, request),
        BudgetCommand::Activate(activation) => super::reconciliation::activate(ledger, activation),
        BudgetCommand::ObserveUsage(observation) => {
            super::reconciliation::observe_validated(ledger, observation)
        }
        BudgetCommand::SettleExact(reference) => {
            super::reconciliation::settle_exact(ledger, reference)
        }
        BudgetCommand::CancelHeld(reference) => {
            super::reconciliation::cancel_held(ledger, reference)
        }
        BudgetCommand::FinalizeAmbiguous(finalization) => {
            super::reconciliation::finalize_ambiguous(ledger, finalization)
        }
        BudgetCommand::Seal(budget_id) => super::lifecycle::seal_validated(ledger, budget_id),
        BudgetCommand::Close(budget_id) => super::lifecycle::close_validated(ledger, budget_id),
    }
}

} // verus!
