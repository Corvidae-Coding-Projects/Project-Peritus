//! Fail-closed aggregation of exact command observations.

use std::path::PathBuf;

use super::TargetGatePlan;

/// One completed target gate command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateExecutionRecord {
    /// Exact display command.
    pub command: String,
    /// Human-readable purpose.
    pub label: String,
    /// Process exit code, absent when the process could not be started.
    pub exit_code: Option<i32>,
    /// Bounded combined output.
    pub output: String,
}

impl GateExecutionRecord {
    /// Whether this command produced an exact successful exit.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.exit_code, Some(0))
    }
}

/// Exact-target gate evidence used by product acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetGateReport {
    changed_paths: Vec<PathBuf>,
    uncovered_paths: Vec<PathBuf>,
    records: Vec<GateExecutionRecord>,
    passed: bool,
}

impl TargetGateReport {
    /// Binds execution records to their complete candidate plan.
    #[must_use]
    pub fn from_execution(plan: &TargetGatePlan, records: Vec<GateExecutionRecord>) -> Self {
        Self::from_execution_with_constraints(plan, records, Vec::new())
    }

    /// Binds planned command results and additional deterministic acceptance constraints.
    ///
    /// Constraints are host-owned checks that depend on request context rather than project
    /// discovery, such as confirming that an explicitly named output path exists. They are
    /// retained beside command records and participate in the same fail-closed decision.
    #[must_use]
    pub fn from_execution_with_constraints(
        plan: &TargetGatePlan,
        mut records: Vec<GateExecutionRecord>,
        constraints: Vec<GateExecutionRecord>,
    ) -> Self {
        let complete = plan.has_complete_coverage() && records.len() == plan.commands().len();
        let passed = complete
            && records.iter().all(GateExecutionRecord::passed)
            && constraints.iter().all(GateExecutionRecord::passed);
        records.extend(constraints);
        Self {
            changed_paths: plan.changed_paths().to_vec(),
            uncovered_paths: plan.uncovered_paths().to_vec(),
            records,
            passed,
        }
    }

    /// Candidate acceptance is impossible unless coverage is complete and all commands pass.
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.passed
    }

    /// Exact changed files covered by this report.
    #[must_use]
    pub fn changed_paths(&self) -> &[PathBuf] {
        &self.changed_paths
    }

    /// Candidate files lacking an executable project contract.
    #[must_use]
    pub fn uncovered_paths(&self) -> &[PathBuf] {
        &self.uncovered_paths
    }

    /// Exact successful and failed command observations.
    #[must_use]
    pub fn records(&self) -> &[GateExecutionRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_constraint_participates_in_acceptance_and_evidence() {
        let root = tempfile::tempdir().expect("root");
        let plan = TargetGatePlan::discover(root.path(), Vec::new()).expect("empty plan");
        let constraint = GateExecutionRecord {
            command: "peritus-internal explicit-output-paths".to_owned(),
            label: "Explicit output paths".to_owned(),
            exit_code: Some(1),
            output: "required output path is missing".to_owned(),
        };

        let report = TargetGateReport::from_execution_with_constraints(
            &plan,
            Vec::new(),
            vec![constraint.clone()],
        );

        assert!(!report.passed());
        assert_eq!(report.records(), &[constraint]);
    }
}
