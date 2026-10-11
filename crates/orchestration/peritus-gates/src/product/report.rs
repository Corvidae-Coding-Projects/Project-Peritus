//! Fail-closed aggregation of exact command observations.

use std::{collections::BTreeMap, path::PathBuf};

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

/// Optional project observations that do not represent selected acceptance checks.
///
/// These remain visible in the final report without being counted as successful gate evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateObservation {
    /// Exact display command associated with the planned observation.
    pub command: String,
    /// Human-readable purpose.
    pub label: String,
    /// Honest `NOT EVALUATED` or `NOT APPLICABLE` explanation.
    pub output: String,
}

/// Exact-target gate evidence used by product acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetGateReport {
    changed_paths: Vec<PathBuf>,
    uncovered_paths: Vec<PathBuf>,
    records: Vec<GateExecutionRecord>,
    observations: Vec<GateObservation>,
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
        records: Vec<GateExecutionRecord>,
        constraints: Vec<GateExecutionRecord>,
    ) -> Self {
        Self::from_execution_with_observations(plan, records, Vec::new(), constraints)
    }

    /// Binds required command results, optional observations, and acceptance constraints to a plan.
    ///
    /// Optional observations account for planned project checks that do not apply to this request,
    /// such as a schema-specific verification when no schema acceptance was selected. They remain
    /// separate from required records and cannot satisfy one.
    #[must_use]
    pub fn from_execution_with_observations(
        plan: &TargetGatePlan,
        mut records: Vec<GateExecutionRecord>,
        observations: Vec<GateObservation>,
        constraints: Vec<GateExecutionRecord>,
    ) -> Self {
        let complete = plan.has_complete_coverage()
            && command_results_cover_plan(plan, &records, &observations);
        let passed = complete
            && records.iter().all(GateExecutionRecord::passed)
            && constraints.iter().all(GateExecutionRecord::passed);
        records.extend(constraints);
        Self {
            changed_paths: plan.changed_paths().to_vec(),
            uncovered_paths: plan.uncovered_paths().to_vec(),
            records,
            observations,
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

    /// Planned checks that were honestly reported as optional or not applicable.
    #[must_use]
    pub fn observations(&self) -> &[GateObservation] {
        &self.observations
    }
}

fn command_results_cover_plan(
    plan: &TargetGatePlan,
    records: &[GateExecutionRecord],
    observations: &[GateObservation],
) -> bool {
    let mut remaining = BTreeMap::<String, usize>::new();
    let mut optional = BTreeMap::<String, usize>::new();
    for command in plan.commands() {
        let display = command.display();
        *remaining.entry(display.clone()).or_default() += 1;
        if command.optional_when_not_selected() {
            *optional.entry(display).or_default() += 1;
        }
    }
    for record in records {
        let Some(count) = remaining.get_mut(&record.command) else { return false };
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    for observation in observations {
        let output = observation.output.trim_start();
        if !output.starts_with("NOT EVALUATED") && !output.starts_with("NOT APPLICABLE") {
            return false;
        }
        let Some(optional_count) = optional.get_mut(&observation.command) else { return false };
        if *optional_count == 0 {
            return false;
        }
        *optional_count -= 1;
        let Some(count) = remaining.get_mut(&observation.command) else { return false };
        if *count == 0 {
            return false;
        }
        *count -= 1;
    }
    remaining.values().all(|count| *count == 0)
}
