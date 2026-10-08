//! Fail-closed aggregation of exact command observations.

use std::path::{Component, PathBuf};

use super::TargetGatePlan;
use crate::{GateError, GateRejection, reject};

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

    /// Restores an exact retained report after its outer candidate and execution binding has been
    /// verified by the product-run continuation reader.
    ///
    /// # Errors
    ///
    /// Rejects noncanonical or unsafe paths and any positive outcome that its retained evidence
    /// cannot justify. A negative outcome remains negative even when every retained command
    /// passed because the original plan may have lacked complete coverage.
    pub fn from_retained(
        changed_paths: Vec<PathBuf>,
        uncovered_paths: Vec<PathBuf>,
        records: Vec<GateExecutionRecord>,
        passed: bool,
    ) -> Result<Self, GateError> {
        if !canonical_paths(&changed_paths)
            || !canonical_paths(&uncovered_paths)
            || uncovered_paths.iter().any(|path| changed_paths.binary_search(path).is_err())
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "retained gate report paths are unsafe or noncanonical",
            ));
        }
        if passed
            && (changed_paths.is_empty()
                || !uncovered_paths.is_empty()
                || records.is_empty()
                || records.iter().any(|record| !record.passed()))
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "retained passing gate report is not supported by complete positive evidence",
            ));
        }
        Ok(Self { changed_paths, uncovered_paths, records, passed })
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

fn canonical_paths(paths: &[PathBuf]) -> bool {
    paths.windows(2).all(|pair| pair[0] < pair[1])
        && paths.iter().all(|path| {
            !path.as_os_str().is_empty()
                && path.components().all(|component| matches!(component, Component::Normal(_)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_constraint_participates_in_acceptance_and_evidence() {
        let root = tempfile::tempdir().expect("root");
        let plan = TargetGatePlan::discover(root.path(), Vec::new(), &[]).expect("empty plan");
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
