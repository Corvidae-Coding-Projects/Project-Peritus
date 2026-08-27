//! Immutable baseline manifests and regression result vocabulary.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{Metric, QualificationError, Sha256Digest, StableId, Statistic};

/// One statistic retained from an accepted prior qualification run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BaselineEntry {
    workload_id: StableId,
    metric: Metric,
    statistic: Statistic,
    value: u64,
    sample_count: usize,
}

impl BaselineEntry {
    /// Constructs a sample-backed baseline statistic.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the sample count is zero or the value is invalid for
    /// the metric's unit.
    pub fn new(
        workload_id: StableId,
        metric: Metric,
        statistic: Statistic,
        value: u64,
        sample_count: usize,
    ) -> Result<Self, QualificationError> {
        if sample_count == 0 {
            return Err(QualificationError::invalid_value(
                "baseline.sample_count",
                "must be greater than zero",
            ));
        }
        metric.validate_value(value)?;
        Ok(Self { workload_id, metric, statistic, value, sample_count })
    }

    /// Returns the workload binding.
    #[must_use]
    pub const fn workload_id(&self) -> &StableId {
        &self.workload_id
    }

    /// Returns the baseline metric.
    #[must_use]
    pub const fn metric(&self) -> Metric {
        self.metric
    }

    /// Returns the retained statistic.
    #[must_use]
    pub const fn statistic(&self) -> Statistic {
        self.statistic
    }

    /// Returns the prior integer statistic value.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }

    /// Returns the source sample count.
    #[must_use]
    pub const fn sample_count(&self) -> usize {
        self.sample_count
    }
}

/// Evidence-bound accepted baseline for one profile and subject revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BaselineManifest {
    id: StableId,
    profile_id: StableId,
    subject_revision: String,
    evidence_digest: Sha256Digest,
    entries: Vec<BaselineEntry>,
}

impl BaselineManifest {
    /// Constructs a baseline and rejects duplicate workload/metric/statistic keys.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the revision is invalid, no entries are supplied, or
    /// two entries use the same workload, metric, and statistic key.
    pub fn new(
        id: StableId,
        profile_id: StableId,
        subject_revision: impl Into<String>,
        evidence_digest: Sha256Digest,
        mut entries: Vec<BaselineEntry>,
    ) -> Result<Self, QualificationError> {
        let subject_revision = subject_revision.into();
        if subject_revision.trim().is_empty() || subject_revision.len() > 200 {
            return Err(QualificationError::invalid_value(
                "baseline.subject_revision",
                "must contain 1 through 200 bytes",
            ));
        }
        if entries.is_empty() {
            return Err(QualificationError::invalid_value("baseline.entries", "must not be empty"));
        }
        entries.sort_by_key(|entry| (entry.workload_id.clone(), entry.metric, entry.statistic));
        let mut seen = BTreeSet::new();
        for entry in &entries {
            let key = (entry.workload_id.clone(), entry.metric, entry.statistic);
            if !seen.insert(key) {
                return Err(QualificationError::Duplicate {
                    kind: "baseline entry",
                    id: format!("{}:{:?}:{:?}", entry.workload_id, entry.metric, entry.statistic),
                });
            }
        }
        Ok(Self { id, profile_id, subject_revision, evidence_digest, entries })
    }

    /// Returns the stable baseline identifier.
    #[must_use]
    pub const fn id(&self) -> &StableId {
        &self.id
    }

    /// Returns the exact profile binding.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile_id
    }

    /// Returns the source subject revision.
    #[must_use]
    pub fn subject_revision(&self) -> &str {
        &self.subject_revision
    }

    /// Returns the digest of the accepted source evidence manifest.
    #[must_use]
    pub const fn evidence_digest(&self) -> &Sha256Digest {
        &self.evidence_digest
    }

    /// Returns entries in stable key order.
    #[must_use]
    pub fn entries(&self) -> &[BaselineEntry] {
        &self.entries
    }

    /// Finds an exact workload, metric, and statistic baseline.
    #[must_use]
    pub fn find(
        &self,
        workload_id: &StableId,
        metric: Metric,
        statistic: Statistic,
    ) -> Option<&BaselineEntry> {
        self.entries.iter().find(|entry| {
            entry.workload_id() == workload_id
                && entry.metric() == metric
                && entry.statistic() == statistic
        })
    }
}

/// Candidate relationship to an exact prior baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegressionClass {
    /// Candidate and baseline are equal or the delta is below the materiality floor.
    Stable,
    /// Candidate is materially better in the metric's declared direction.
    Improvement,
    /// Candidate is worse by at least the warning threshold.
    Warning,
    /// Candidate is worse by at least the blocking threshold.
    Blocking,
    /// No exact baseline entry was available.
    Incomparable,
}

/// Exact baseline comparison for one objective statistic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RegressionResult {
    objective_id: StableId,
    class: RegressionClass,
    baseline_value: Option<u64>,
    candidate_value: Option<u64>,
    absolute_delta: Option<u64>,
    relative_basis_points: Option<u64>,
}

impl RegressionResult {
    pub(crate) const fn new(
        objective_id: StableId,
        class: RegressionClass,
        baseline_value: Option<u64>,
        candidate_value: Option<u64>,
        absolute_delta: Option<u64>,
        relative_basis_points: Option<u64>,
    ) -> Self {
        Self {
            objective_id,
            class,
            baseline_value,
            candidate_value,
            absolute_delta,
            relative_basis_points,
        }
    }

    /// Returns the objective being compared.
    #[must_use]
    pub const fn objective_id(&self) -> &StableId {
        &self.objective_id
    }

    /// Returns the regression classification.
    #[must_use]
    pub const fn class(&self) -> RegressionClass {
        self.class
    }

    /// Returns prior value when an exact baseline existed.
    #[must_use]
    pub const fn baseline_value(&self) -> Option<u64> {
        self.baseline_value
    }

    /// Returns candidate value when sufficient samples existed.
    #[must_use]
    pub const fn candidate_value(&self) -> Option<u64> {
        self.candidate_value
    }

    /// Returns absolute difference when both values existed.
    #[must_use]
    pub const fn absolute_delta(&self) -> Option<u64> {
        self.absolute_delta
    }

    /// Returns absolute difference relative to the baseline in basis points.
    #[must_use]
    pub const fn relative_basis_points(&self) -> Option<u64> {
        self.relative_basis_points
    }
}
