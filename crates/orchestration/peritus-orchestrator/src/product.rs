//! Production-facing E0 decision composition.

use peritus_gates::TargetGateReport;
use peritus_review::ProductFindingLedger;

/// Next effect or terminal selected from exact D1 and D2 observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionDecision {
    /// Exact candidate gates passed and no conserved policy blocker remains.
    Accept,
    /// Run a fixer with gate failures and every conserved open finding.
    Fix,
    /// Explicit cycle budget was exhausted while acceptance remained impossible.
    Exhausted,
}

/// Small production adapter over E0's writer-gates-review-fixer acceptance invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionRunCoordinator {
    max_fixer_cycles: u32,
    completed_fixer_cycles: u32,
}

impl ProductionRunCoordinator {
    /// Creates an explicit nonzero fixer bound.
    ///
    /// # Errors
    /// Rejects zero or an impractically wide cycle budget.
    pub const fn new(max_fixer_cycles: u32) -> Result<Self, &'static str> {
        if max_fixer_cycles == 0 || max_fixer_cycles > 32 {
            Err("production fixer cycle bound is invalid")
        } else {
            Ok(Self { max_fixer_cycles, completed_fixer_cycles: 0 })
        }
    }

    /// Derives the only legal next step from fresh D1/D2 evidence.
    #[must_use]
    pub fn decide(
        &self,
        gates: &TargetGateReport,
        findings: &ProductFindingLedger,
    ) -> ProductionDecision {
        if gates.passed() && !findings.has_blockers() {
            ProductionDecision::Accept
        } else if self.completed_fixer_cycles < self.max_fixer_cycles {
            ProductionDecision::Fix
        } else {
            ProductionDecision::Exhausted
        }
    }

    /// Records one completed fixer effect. It grants no acceptance and closes no finding.
    pub const fn record_fixer_completed(&mut self) {
        self.completed_fixer_cycles = self.completed_fixer_cycles.saturating_add(1);
    }

    /// Completed fixer cycles.
    #[must_use]
    pub const fn completed_fixer_cycles(&self) -> u32 {
        self.completed_fixer_cycles
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use peritus_gates::{GateExecutionRecord, TargetGatePlan, TargetGateReport};
    use peritus_review::ProductFindingLedger;

    use super::*;

    #[test]
    fn complete_is_impossible_without_exact_changed_target_evidence() {
        let root = tempfile::tempdir().expect("root");
        let plan =
            TargetGatePlan::discover(root.path(), vec![PathBuf::from("uncovered/new-file.txt")])
                .expect("plan");
        let report = TargetGateReport::from_execution(&plan, Vec::<GateExecutionRecord>::new());
        let coordinator = ProductionRunCoordinator::new(2).expect("coordinator");
        assert_eq!(
            coordinator.decide(&report, &ProductFindingLedger::new()),
            ProductionDecision::Fix,
        );
    }
}
