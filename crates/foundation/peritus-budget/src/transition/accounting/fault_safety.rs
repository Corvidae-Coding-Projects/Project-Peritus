//! Executable reachability witnesses for infallible ancestor fault propagation.

use crate::BudgetLedger;
use peritus_types::BudgetId;

pub(in crate::transition) fn establish_fault_lineage_safe(
    ledger: &BudgetLedger,
    index: usize,
    current_id: BudgetId,
    fuel: usize,
) {
    match ledger.accounts[index].parent_id {
        None => {}
        Some(parent_id) => {
            let parent_index = match super::find_account(ledger, parent_id) {
                Some(parent_index) => parent_index,
                None => {
                    return;
                }
            };
            establish_fault_lineage_safe(ledger, parent_index, parent_id, fuel - 1);
        }
    }
}
