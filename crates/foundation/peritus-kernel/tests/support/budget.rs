//! B1 budget fixtures used by kernel integration tests.

use super::Fixture;
use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetSnapshot, ChildBudgetRequest,
};
use peritus_types::{RevisionNumber, RevisionTuple};

impl Fixture {
    pub fn budget_snapshots(&self) -> (BudgetSnapshot, BudgetSnapshot) {
        let root_limits = BudgetLimits::new(BudgetAmounts::from_units(100, 1_000, 60_000, 10, 5));
        let ledger = BudgetLedger::new_root(self.root_budget_id, self.revision, root_limits);
        let transition = ledger
            .transition(BudgetCommand::AllocateChild(ChildBudgetRequest::new(
                self.child_budget_id,
                self.root_budget_id,
                self.revision,
                BudgetLimits::new(BudgetAmounts::from_units(40, 400, 20_000, 4, 2)),
            )))
            .expect("child allocation");
        let ledger = transition.into_ledger();
        (
            ledger.account(self.root_budget_id).expect("root snapshot"),
            ledger.account(self.child_budget_id).expect("child snapshot"),
        )
    }

    pub fn root_budget_snapshot(&self, revision: RevisionTuple, closed: bool) -> BudgetSnapshot {
        let limits = BudgetLimits::new(BudgetAmounts::from_units(100, 1_000, 60_000, 10, 5));
        let ledger = BudgetLedger::new_root(self.root_budget_id, revision, limits);
        let ledger = if closed {
            let sealed = ledger
                .transition(BudgetCommand::Seal(self.root_budget_id))
                .expect("seal root budget")
                .into_ledger();
            sealed
                .transition(BudgetCommand::Close(self.root_budget_id))
                .expect("close root budget")
                .into_ledger()
        } else {
            ledger
        };
        ledger.account(self.root_budget_id).expect("root snapshot")
    }

    pub fn alternate_revision(&self) -> RevisionTuple {
        RevisionTuple::new(
            self.revision.acceptance_spec_id(),
            self.revision.harness_id(),
            self.revision.workspace_id(),
            self.revision.workspace_generation(),
            RevisionNumber::new(2).expect("alternate revision"),
            self.revision.policy_id(),
            self.revision.provider_profile_id(),
        )
    }
}
