//! Ancestor fault propagation for ambiguous or exceeded reservations.

use crate::{BudgetAccountPhase, BudgetError, BudgetLedger};
use peritus_types::BudgetId;

pub(in crate::transition) fn fault_lineage(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> Result<(), BudgetError> {
    let account_count = ledger.accounts.len();

    fault_lineage_inner(ledger, budget_id, account_count)
}
fn fault_lineage_inner(
    ledger: &mut BudgetLedger,
    current_id: BudgetId,
    remaining_steps: usize,
) -> Result<(), BudgetError> {
    if remaining_steps == 0 {
        return Err(crate::model::corrupt(current_id));
    }
    let index = match super::find_account(ledger, current_id) {
        Some(index) => index,
        None => {
            return Err(crate::model::corrupt(current_id));
        }
    };
    let parent_id = ledger.accounts[index].parent_id;
    ledger.accounts[index].phase = BudgetAccountPhase::Faulted;
    if let Some(parent_id) = parent_id {
        fault_lineage_inner(ledger, parent_id, remaining_steps - 1)?;
    }
    Ok(())
}
