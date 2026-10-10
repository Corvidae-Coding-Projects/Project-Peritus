//! Ancestor consumption-propagation validation.

use super::account_at;
use crate::{BudgetError, BudgetLedger};

pub(super) fn validate_ancestor_propagation(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> Result<(), BudgetError> {
    if before.accounts.len() > after.accounts.len() {
        return Err(crate::model::corrupt(after.root_id));
    }
    let mut child_index = 0;
    while child_index < before.accounts.len() {
        validate_child_propagation(before, after, child_index)?;
        child_index += 1;
    }
    Ok(())
}
fn validate_child_propagation(
    before: &BudgetLedger,
    after: &BudgetLedger,
    child_index: usize,
) -> Result<(), BudgetError> {
    let prior_child = account_at(before, child_index)?;
    let next_child = account_at(after, child_index)?;
    let changed = !prior_child.consumed.equals(next_child.consumed);
    if prior_child.parent_id.is_some() && changed {
        let parent_index = find_parent_index(before, child_index)?;
        let prior_parent = account_at(before, parent_index)?;
        let next_parent = account_at(after, parent_index)?;
        let child_delta = next_child
            .consumed
            .checked_sub(prior_child.consumed)
            .map_err(BudgetError::arithmetic)?;
        let parent_delta = next_parent
            .consumed
            .checked_sub(prior_parent.consumed)
            .map_err(BudgetError::arithmetic)?;
        if !child_delta.equals(parent_delta) {
            return Err(crate::model::corrupt(prior_child.id));
        }
    }
    Ok(())
}
fn find_parent_index(ledger: &BudgetLedger, child_index: usize) -> Result<usize, BudgetError> {
    if child_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    if child_index > 0 {
        super::super::structure::validate_parent_before(ledger, child_index)?;
    }
    let mut index = 0;
    while index < ledger.accounts.len() {
        if crate::identity_model::parent_matches_id(
            ledger.accounts[child_index].parent_id,
            ledger.accounts[index].id,
        ) {
            return Ok(index);
        }
        index += 1;
    }
    Err(crate::model::corrupt(ledger.accounts[child_index].id))
}
