//! Deterministic B1 port used by D0 integration tests.
#![allow(dead_code, reason = "each integration target uses a different fixture subset")]

use peritus_agent::{
    AgentBudgetPlan, AgentBudgetPort, AgentBudgetPortError, AgentBudgetReservation,
};
use peritus_budget::{BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetReceipt};
use peritus_types::{ActionId, BudgetId, BudgetReservationId, RevisionTuple, Sha256Digest};

pub struct LedgerBudgetPort {
    ledger: BudgetLedger,
}

impl LedgerBudgetPort {
    pub fn new(root: BudgetId, revision: RevisionTuple, limits: BudgetAmounts) -> Self {
        Self { ledger: BudgetLedger::new_root(root, revision, BudgetLimits::new(limits)) }
    }

    pub const fn ledger(&self) -> &BudgetLedger {
        &self.ledger
    }
}

impl AgentBudgetPort for LedgerBudgetPort {
    fn commit(&mut self, command: BudgetCommand) -> Result<BudgetReceipt, AgentBudgetPortError> {
        let transition = self.ledger.transition(command).map_err(AgentBudgetPortError::from)?;
        let (ledger, receipt) = transition.into_parts();
        self.ledger = ledger;
        Ok(receipt)
    }
}

pub fn model_budget(
    seed: u8,
    revision: RevisionTuple,
) -> (LedgerBudgetPort, AgentBudgetReservation) {
    let root = BudgetId::new([seed; 16]).expect("budget id");
    let mut port = LedgerBudgetPort::new(root, revision, BudgetAmounts::from_units(0, 0, 1, 1, 0));
    let plan = AgentBudgetPlan::new(
        BudgetReservationId::new([seed.wrapping_add(1); 16]).expect("reservation"),
        root,
        revision,
        ActionId::new([seed.wrapping_add(2); 16]).expect("action"),
        Sha256Digest::new([seed.wrapping_add(3); 32]),
        BudgetAmounts::from_units(0, 0, 1, 0, 0),
        false,
    )
    .expect("budget plan");
    let reservation = AgentBudgetReservation::begin(&mut port, plan).expect("begin budget");
    (port, reservation)
}
