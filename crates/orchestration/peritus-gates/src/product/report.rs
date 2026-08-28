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
        let complete = plan.has_complete_coverage() && records.len() == plan.commands().len();
        let passed = complete && records.iter().all(GateExecutionRecord::passed);
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
