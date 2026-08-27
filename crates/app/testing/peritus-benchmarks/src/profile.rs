//! Validated machine, resource, regression, and qualification profiles.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::profile_resources::{ReferenceMachine, ResourceEnvelope};
use crate::{QualificationError, SloObjective, StableId};

/// Relative and absolute thresholds used to classify baseline regressions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RegressionPolicy {
    warning_basis_points: u16,
    blocking_basis_points: u16,
    minimum_absolute_delta: u64,
    baseline_required: bool,
}

impl RegressionPolicy {
    /// Constructs a policy whose blocking threshold is no smaller than its warning threshold.
    ///
    /// # Errors
    /// Returns [`QualificationError`] for out-of-range or incorrectly ordered thresholds.
    pub const fn new(
        warning_basis_points: u16,
        blocking_basis_points: u16,
        minimum_absolute_delta: u64,
        baseline_required: bool,
    ) -> Result<Self, QualificationError> {
        if warning_basis_points > 10_000
            || blocking_basis_points > 10_000
            || blocking_basis_points < warning_basis_points
        {
            return Err(QualificationError::invalid_value(
                "regression_policy",
                "basis points must be ordered and at most 10,000",
            ));
        }
        Ok(Self {
            warning_basis_points,
            blocking_basis_points,
            minimum_absolute_delta,
            baseline_required,
        })
    }

    /// Returns the warning regression threshold.
    #[must_use]
    pub const fn warning_basis_points(self) -> u16 {
        self.warning_basis_points
    }

    /// Returns the blocking regression threshold.
    #[must_use]
    pub const fn blocking_basis_points(self) -> u16 {
        self.blocking_basis_points
    }

    /// Returns the minimum absolute metric-unit delta considered material.
    #[must_use]
    pub const fn minimum_absolute_delta(self) -> u64 {
        self.minimum_absolute_delta
    }

    /// Returns whether absence of a matching baseline blocks readiness.
    #[must_use]
    pub const fn baseline_required(self) -> bool {
        self.baseline_required
    }
}

/// Builder that admits only complete, internally consistent qualification profiles.
pub struct QualificationProfileBuilder {
    id: StableId,
    description: String,
    reference_machine: ReferenceMachine,
    envelope: ResourceEnvelope,
    objectives: Vec<SloObjective>,
    required_workloads: BTreeSet<StableId>,
    max_measurements: usize,
    regression_policy: RegressionPolicy,
}

impl QualificationProfileBuilder {
    /// Starts a profile with conservative explicit limits and no implicit objectives.
    pub fn new(
        id: StableId,
        description: impl Into<String>,
        reference_machine: ReferenceMachine,
        envelope: ResourceEnvelope,
        regression_policy: RegressionPolicy,
    ) -> Self {
        Self {
            id,
            description: description.into(),
            reference_machine,
            envelope,
            objectives: Vec::new(),
            required_workloads: BTreeSet::new(),
            max_measurements: 1_000_000,
            regression_policy,
        }
    }

    /// Adds one workload-scoped objective.
    #[must_use]
    pub fn objective(mut self, objective: SloObjective) -> Self {
        self.required_workloads.insert(objective.workload_id().clone());
        self.objectives.push(objective);
        self
    }

    /// Requires workload coverage even when no objective selects it directly.
    #[must_use]
    pub fn required_workload(mut self, workload_id: StableId) -> Self {
        self.required_workloads.insert(workload_id);
        self
    }

    /// Sets the hard measurement-ingestion bound.
    ///
    /// # Errors
    /// Returns [`QualificationError`] when the measurement bound is zero.
    pub fn max_measurements(mut self, max_measurements: usize) -> Result<Self, QualificationError> {
        if max_measurements == 0 {
            return Err(QualificationError::invalid_value(
                "profile.max_measurements",
                "must be greater than zero",
            ));
        }
        self.max_measurements = max_measurements;
        Ok(self)
    }

    /// Validates uniqueness and produces a qualification profile.
    ///
    /// # Errors
    /// Returns [`QualificationError`] for invalid text, absent objectives, or duplicate IDs.
    pub fn build(mut self) -> Result<QualificationProfile, QualificationError> {
        if self.description.trim().is_empty() || self.description.len() > 512 {
            return Err(QualificationError::invalid_value(
                "profile.description",
                "must contain 1 through 512 bytes",
            ));
        }
        if self.objectives.is_empty() {
            return Err(QualificationError::invalid_value(
                "profile.objectives",
                "at least one objective is required",
            ));
        }
        self.objectives.sort_by(|left, right| left.id().cmp(right.id()));
        for pair in self.objectives.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(QualificationError::Duplicate {
                    kind: "objective",
                    id: pair[0].id().to_string(),
                });
            }
        }
        Ok(QualificationProfile {
            id: self.id,
            description: self.description,
            reference_machine: self.reference_machine,
            envelope: self.envelope,
            objectives: self.objectives,
            required_workloads: self.required_workloads.into_iter().collect(),
            max_measurements: self.max_measurements,
            regression_policy: self.regression_policy,
        })
    }
}

/// Immutable validated performance qualification profile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QualificationProfile {
    id: StableId,
    description: String,
    reference_machine: ReferenceMachine,
    envelope: ResourceEnvelope,
    objectives: Vec<SloObjective>,
    required_workloads: Vec<StableId>,
    max_measurements: usize,
    regression_policy: RegressionPolicy,
}

impl QualificationProfile {
    /// Returns the stable profile identifier.
    #[must_use]
    pub const fn id(&self) -> &StableId {
        &self.id
    }

    /// Returns the human-readable profile purpose.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the exact reference-machine contract.
    #[must_use]
    pub const fn reference_machine(&self) -> &ReferenceMachine {
        &self.reference_machine
    }

    /// Returns enforced resource bounds.
    #[must_use]
    pub const fn envelope(&self) -> ResourceEnvelope {
        self.envelope
    }

    /// Returns objectives in stable identifier order.
    #[must_use]
    pub fn objectives(&self) -> &[SloObjective] {
        &self.objectives
    }

    /// Returns required workload identifiers in stable order.
    #[must_use]
    pub fn required_workloads(&self) -> &[StableId] {
        &self.required_workloads
    }

    /// Returns the hard measurement record bound.
    #[must_use]
    pub const fn max_measurements(&self) -> usize {
        self.max_measurements
    }

    /// Returns baseline regression policy.
    #[must_use]
    pub const fn regression_policy(&self) -> RegressionPolicy {
        self.regression_policy
    }
}
