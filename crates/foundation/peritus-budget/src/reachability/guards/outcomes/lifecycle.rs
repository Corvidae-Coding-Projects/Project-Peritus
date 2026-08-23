//! Runtime bridge for exact seal and close outcome guards.

#[cfg(verus_only)]
use crate::{BudgetAccountPhase, BudgetLedger, BudgetReceiptKind};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) proof fn lifecycle_guard_from_runtime(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    kind: BudgetReceiptKind,
    close: bool,
    index: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        crate::reachability::guards::account_at(ledger, budget_id, index),
        if close {
            match kind {
                BudgetReceiptKind::Idempotent => {
                    ledger.accounts[index].phase == BudgetAccountPhase::Closed
                }
                BudgetReceiptKind::Applied => {
                    (ledger.accounts[index].phase == BudgetAccountPhase::Draining
                        || ledger.accounts[index].phase == BudgetAccountPhase::Faulted)
                        && crate::invariant::budget_has_no_live_work(ledger, budget_id)
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
        } else {
            match kind {
                BudgetReceiptKind::Applied => {
                    ledger.accounts[index].phase == BudgetAccountPhase::Open
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.accounts[index].phase != BudgetAccountPhase::Open
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
        },
    ensures crate::reachability::guards::accepted_command_guard(
        ledger,
        if close {
            crate::BudgetCommand::Close(budget_id)
        } else {
            crate::BudgetCommand::Seal(budget_id)
        },
        kind,
    ),
{
    assert(exists |witness: int| #![auto]
        crate::reachability::guards::account_at(ledger, budget_id, witness)
            && if close {
                match kind {
                    BudgetReceiptKind::Idempotent => {
                        ledger.accounts[witness].phase == BudgetAccountPhase::Closed
                    }
                    BudgetReceiptKind::Applied => {
                        (ledger.accounts[witness].phase == BudgetAccountPhase::Draining
                            || ledger.accounts[witness].phase == BudgetAccountPhase::Faulted)
                            && crate::invariant::budget_has_no_live_work(ledger, budget_id)
                    }
                    BudgetReceiptKind::OverrunFaulted => false,
                }
            } else {
                match kind {
                    BudgetReceiptKind::Applied => {
                        ledger.accounts[witness].phase == BudgetAccountPhase::Open
                    }
                    BudgetReceiptKind::Idempotent => {
                        ledger.accounts[witness].phase != BudgetAccountPhase::Open
                    }
                    BudgetReceiptKind::OverrunFaulted => false,
                }
            });
}

} // verus!
