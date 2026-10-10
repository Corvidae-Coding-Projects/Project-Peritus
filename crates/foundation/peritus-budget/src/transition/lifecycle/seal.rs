//! Account sealing transitions.

use super::super::accounting::{receipt, require_account};
use crate::{
    BudgetAccountPhase, BudgetError, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind,
};
use peritus_types::BudgetId;

pub(super) fn seal(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> Result<BudgetReceipt, BudgetError> {
    super::super::validation::validate(ledger)?;
    seal_validated(ledger, budget_id)
}
pub(in crate::transition) fn seal_validated(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> Result<BudgetReceipt, BudgetError> {
    let account_index = match require_account(ledger, budget_id) {
        Ok(index) => index,
        Err(error) => {
            return Err(error);
        }
    };
    let kind = match ledger.accounts[account_index].phase {
        BudgetAccountPhase::Open => {
            ledger.accounts[account_index].phase = BudgetAccountPhase::Draining;
            BudgetReceiptKind::Applied
        }
        BudgetAccountPhase::Draining | BudgetAccountPhase::Faulted | BudgetAccountPhase::Closed => {
            BudgetReceiptKind::Idempotent
        }
    };
    let seal_receipt = receipt(BudgetOperation::Seal, kind, budget_id);
    Ok(seal_receipt)
}
