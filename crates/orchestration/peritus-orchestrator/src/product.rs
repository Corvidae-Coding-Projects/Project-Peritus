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
}

/// Small production adapter over E0's writer-gates-review-fixer acceptance invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionRunCoordinator {
    completed_fixer_cycles: u32,
}

impl ProductionRunCoordinator {
    /// Creates a coordinator with the exact number of already completed fixer cycles.
    #[must_use]
    pub const fn new(completed_fixer_cycles: u32) -> Self {
        Self { completed_fixer_cycles }
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
        } else {
            ProductionDecision::Fix
        }
    }

    /// Selects the next step for caller-authorized external effects.
    ///
    /// This path does not reinterpret an empty workspace gate as passing. The caller must instead
    /// supply a complete, independently reviewable external-effect evidence decision.
    #[must_use]
    pub fn decide_external_effects(
        &self,
        evidence_complete: bool,
        findings: &ProductFindingLedger,
    ) -> ProductionDecision {
        if evidence_complete && !findings.has_blockers() {
            ProductionDecision::Accept
        } else {
            ProductionDecision::Fix
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
