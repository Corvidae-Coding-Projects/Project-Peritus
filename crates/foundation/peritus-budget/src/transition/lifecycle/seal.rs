//! Account sealing transitions.

use super::super::accounting::{receipt, require_account};
use crate::{
    BudgetAccountPhase, BudgetError, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind,
};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(super) fn seal(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<BudgetReceipt, BudgetError>)
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Seal(budget_id),
                final(ledger),
                receipt,
            ),
            Err(_) => true,
        },
{
    super::super::validation::validate(ledger)?;
    seal_validated(ledger, budget_id)
}

pub(in crate::transition) fn seal_validated(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Seal(budget_id),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::Seal(budget_id),
                error,
            ),
        },
{
    let ghost before = *ledger;
    proof {
        assert(crate::model::ledger_well_formed(&before));
    }
    let account_index = match require_account(ledger, budget_id) {
        Ok(index) => index,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Seal(budget_id),
                error,
            ));
            return Err(error);
        }
    };
    let kind = match ledger.accounts[account_index].phase {
        BudgetAccountPhase::Open => {
            assert(crate::reachability::account_at_guard(
                &before,
                budget_id,
                account_index as int,
            ));
            ledger.accounts[account_index].phase = BudgetAccountPhase::Draining;
            BudgetReceiptKind::Applied
        }
        BudgetAccountPhase::Draining | BudgetAccountPhase::Faulted | BudgetAccountPhase::Closed => {
            BudgetReceiptKind::Idempotent
        }
    };
    let seal_receipt = receipt(BudgetOperation::Seal, kind, budget_id);
    proof {
        if kind == BudgetReceiptKind::Applied {
            assert(ledger.accounts@ == before.accounts@.update(
                account_index as int,
                ledger.accounts[account_index as int],
            ));
            crate::reachability::account_phase_effect_from_record(
                &before,
                ledger,
                budget_id,
                BudgetAccountPhase::Draining,
                account_index as int,
            );
            assert(crate::reachability::seal_effect_exact(&before, ledger, budget_id));
        } else {
            crate::reachability::ledger_exact_reflexive(ledger);
            assert(crate::reachability::ledgers_exactly_equal(&before, ledger));
        }
        crate::reachability::lifecycle_guard_from_runtime(
            &before,
            budget_id,
            seal_receipt.spec_kind(),
            false,
            account_index as int,
        );
        crate::reachability::seal_refines(
            &before,
            ledger,
            budget_id,
            seal_receipt,
        );
    }
    Ok(seal_receipt)
}

} // verus!
