//! Aggregate matching relations and proofs over derived accounting folds.

use super::child_contribution_le_sum;

use crate::{BudgetDimension, BudgetLedger};

pub(crate) fn child_unused_le_parent(
    ledger: &BudgetLedger,
    child: usize,
    parent: usize,
    unused: crate::BudgetAmounts,
) {
    unused.establish_bounds();
    let parent_id = ledger.accounts[parent].id;
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::ModelTokens,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::ProviderCostMicrounits,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::ActiveEffectMilliseconds,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::Attempts,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::Retries,
        child,
        ledger.accounts.len(),
    );
}
