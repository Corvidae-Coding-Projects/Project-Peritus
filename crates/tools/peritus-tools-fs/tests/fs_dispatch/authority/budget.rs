//! Exact held tool-budget receipt.

use peritus_budget::{BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetRequest};
use peritus_journal::{
    AggregateKind, BudgetCommitRequest, CommittedBudgetTransition, HeadExpectation, SqliteJournal,
};
use peritus_types::Sha256Digest;

use super::{Ids, journal};

pub fn commit(
    store: &mut SqliteJournal,
    ids: &Ids,
    action_digest: Sha256Digest,
) -> CommittedBudgetTransition {
    const ACTIVE_MILLIS: u64 = 30_000;
    let ledger = BudgetLedger::new_root(
        ids.tool_budget,
        ids.revision,
        BudgetLimits::new(BudgetAmounts::from_units(10, 10, ACTIVE_MILLIS, 2, 1)),
    );
    let request = BudgetRequest::new(
        ids.reservation,
        ids.tool_budget,
        ids.revision,
        ids.action,
        action_digest,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(0, 0, ACTIVE_MILLIS, 0, 0),
    );
    let transition = ledger.transition(BudgetCommand::Begin(request)).expect("budget begin");
    let key = journal::aggregate(AggregateKind::Budget, 80);
    store
        .commit_budget_transition(
            BudgetCommitRequest::new(
                journal::append(
                    key,
                    journal::command(80),
                    1,
                    journal::event(80),
                    None,
                    HeadExpectation::Absent(key),
                    ids.revision,
                ),
                transition,
                None,
                None,
            )
            .expect("bind budget"),
        )
        .expect("commit budget")
}
