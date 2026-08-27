//! Bounded, binding-checked measurement ingestion.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Metric, QualificationError, StableId};

/// One integer-valued observation in the unit fixed by its metric.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MeasurementRecord {
    run_id: StableId,
    profile_id: StableId,
    workload_id: StableId,
    metric: Metric,
    sequence: u64,
    elapsed_micros: u64,
    value: u64,
}

impl MeasurementRecord {
    /// Constructs a typed observation and validates unit-specific bounds.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the value is outside the metric's valid unit range.
    pub fn new(
        run_id: StableId,
        profile_id: StableId,
        workload_id: StableId,
        metric: Metric,
        sequence: u64,
        elapsed_micros: u64,
        value: u64,
    ) -> Result<Self, QualificationError> {
        metric.validate_value(value)?;
        Ok(Self { run_id, profile_id, workload_id, metric, sequence, elapsed_micros, value })
    }

    /// Returns the qualification run binding.
    #[must_use]
    pub const fn run_id(&self) -> &StableId {
        &self.run_id
    }

    /// Returns the qualification profile binding.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile_id
    }

    /// Returns the stable workload binding.
    #[must_use]
    pub const fn workload_id(&self) -> &StableId {
        &self.workload_id
    }

    /// Returns the metric and therefore the value unit.
    #[must_use]
    pub const fn metric(&self) -> Metric {
        self.metric
    }

    /// Returns the contiguous recorder sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns elapsed time since the runner's monotonic origin.
    #[must_use]
    pub const fn elapsed_micros(&self) -> u64 {
        self.elapsed_micros
    }

    /// Returns the integer observation in the metric's declared unit.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }
}

/// Immutable, validated observations for one profile-bound runner invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MeasurementSet {
    run_id: StableId,
    profile_id: StableId,
    records: Vec<MeasurementRecord>,
}

impl MeasurementSet {
    /// Returns the qualification run binding.
    #[must_use]
    pub const fn run_id(&self) -> &StableId {
        &self.run_id
    }

    /// Returns the qualification profile binding.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile_id
    }

    /// Returns records in contiguous recorder order.
    #[must_use]
    pub fn records(&self) -> &[MeasurementRecord] {
        &self.records
    }

    /// Returns workload identifiers represented by at least one record.
    #[must_use]
    pub fn observed_workloads(&self) -> BTreeSet<&StableId> {
        self.records.iter().map(MeasurementRecord::workload_id).collect()
    }
}

/// Bounded sink that validates bindings, sequence, and monotonic elapsed time before retention.
pub struct MeasurementIngestor {
    run_id: StableId,
    profile_id: StableId,
    allowed_workloads: BTreeSet<StableId>,
    max_records: usize,
    next_sequence: u64,
    last_elapsed_micros: Option<u64>,
    records: Vec<MeasurementRecord>,
}

impl MeasurementIngestor {
    /// Creates a sink for one run and the exact workloads admitted by its profile.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the record bound is zero or no workload is admitted.
    pub fn new(
        run_id: StableId,
        profile_id: StableId,
        allowed_workloads: impl IntoIterator<Item = StableId>,
        max_records: usize,
    ) -> Result<Self, QualificationError> {
        if max_records == 0 {
            return Err(QualificationError::invalid_value(
                "measurement.max_records",
                "must be greater than zero",
            ));
        }
        let allowed_workloads = allowed_workloads.into_iter().collect::<BTreeSet<_>>();
        if allowed_workloads.is_empty() {
            return Err(QualificationError::invalid_value(
                "measurement.allowed_workloads",
                "must not be empty",
            ));
        }
        Ok(Self {
            run_id,
            profile_id,
            allowed_workloads,
            max_records,
            next_sequence: 0,
            last_elapsed_micros: None,
            records: Vec::new(),
        })
    }

    /// Validates and retains one observation without mutating the sink on rejection.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for mismatched run/profile/workload bindings, record-limit
    /// exhaustion, a sequence gap, backwards elapsed time, or sequence overflow.
    pub fn record(&mut self, record: MeasurementRecord) -> Result<(), QualificationError> {
        Self::require_binding("run_id", &self.run_id, record.run_id())?;
        Self::require_binding("profile_id", &self.profile_id, record.profile_id())?;
        if !self.allowed_workloads.contains(record.workload_id()) {
            return Err(QualificationError::UnknownReference {
                kind: "workload",
                id: record.workload_id().to_string(),
            });
        }
        if self.records.len() >= self.max_records {
            return Err(QualificationError::MeasurementLimit { limit: self.max_records });
        }
        if record.sequence() != self.next_sequence {
            return Err(QualificationError::MeasurementSequence {
                expected: self.next_sequence,
                observed: record.sequence(),
            });
        }
        if let Some(previous) = self.last_elapsed_micros
            && record.elapsed_micros() < previous
        {
            return Err(QualificationError::NonMonotonicMeasurement {
                field: "elapsed_micros",
                previous,
                observed: record.elapsed_micros(),
            });
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(QualificationError::ArithmeticOverflow("measurement sequence"))?;
        self.last_elapsed_micros = Some(record.elapsed_micros());
        self.records.push(record);
        Ok(())
    }

    /// Parses newline-delimited measurement JSON within a caller-supplied byte bound.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the document exceeds its byte limit, a line is invalid
    /// JSON, or any decoded measurement violates the sink contract.
    pub fn ingest_json_lines(
        &mut self,
        document: &str,
        max_document_bytes: usize,
    ) -> Result<(), QualificationError> {
        if document.len() > max_document_bytes {
            return Err(QualificationError::DocumentLimit {
                kind: "measurement",
                limit: max_document_bytes,
            });
        }
        for (index, line) in document.lines().enumerate() {
            let raw: MeasurementWire = serde_json::from_str(line).map_err(|source| {
                QualificationError::MeasurementJson { line: index + 1, source }
            })?;
            self.record(raw.validate()?)?;
        }
        Ok(())
    }

    /// Seals the accepted records into an immutable set.
    #[must_use]
    pub fn finish(self) -> MeasurementSet {
        MeasurementSet { run_id: self.run_id, profile_id: self.profile_id, records: self.records }
    }

    fn require_binding(
        field: &'static str,
        expected: &StableId,
        observed: &StableId,
    ) -> Result<(), QualificationError> {
        if expected == observed {
            Ok(())
        } else {
            Err(QualificationError::MeasurementBinding {
                field,
                expected: expected.to_string(),
                observed: observed.to_string(),
            })
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MeasurementWire {
    run_id: String,
    profile_id: String,
    workload_id: String,
    metric: Metric,
    sequence: u64,
    elapsed_micros: u64,
    value: u64,
}

impl MeasurementWire {
    fn validate(self) -> Result<MeasurementRecord, QualificationError> {
        MeasurementRecord::new(
            StableId::new(self.run_id)?,
            StableId::new(self.profile_id)?,
            StableId::new(self.workload_id)?,
            self.metric,
            self.sequence,
            self.elapsed_micros,
            self.value,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{MeasurementRecord, Metric, StableId};

    use super::MeasurementIngestor;

    #[test]
    fn rejected_sequence_does_not_advance_the_sink() {
        let run = StableId::new("run").expect("id");
        let profile = StableId::new("profile").expect("id");
        let workload = StableId::new("workload").expect("id");
        let mut sink =
            MeasurementIngestor::new(run.clone(), profile.clone(), [workload.clone()], 2)
                .expect("sink");
        let skipped = MeasurementRecord::new(
            run.clone(),
            profile.clone(),
            workload.clone(),
            Metric::EventAppendLatency,
            1,
            1,
            1,
        )
        .expect("record");
        assert!(sink.record(skipped).is_err());
        let first =
            MeasurementRecord::new(run, profile, workload, Metric::EventAppendLatency, 0, 1, 1)
                .expect("record");
        sink.record(first).expect("record accepted");
    }
}
