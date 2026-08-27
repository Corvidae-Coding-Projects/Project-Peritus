//! Bounded JSON loading for accepted baseline manifests.

use serde::Deserialize;

use crate::{
    BaselineEntry, BaselineManifest, DatasetLimits, Metric, QualificationError, Sha256Digest,
    StableId, Statistic,
};

/// Decodes and validates a bounded accepted-baseline document.
///
/// # Errors
///
/// Returns [`QualificationError`] when the document exceeds its limit, cannot be decoded, uses an
/// unsupported schema, exceeds entry bounds, or contains invalid baseline data.
pub fn baseline_from_json(
    document: &str,
    limits: DatasetLimits,
) -> Result<BaselineManifest, QualificationError> {
    require_document_limit(document, limits.baseline_bytes())?;
    let raw: BaselineWire = serde_json::from_str(document)
        .map_err(|source| QualificationError::Json { kind: "baseline", source })?;
    if raw.entries.len() > limits.max_objectives() {
        return Err(QualificationError::invalid_value(
            "baseline.entries",
            "entry count exceeds configured dataset limits",
        ));
    }
    raw.validate()
}

const fn require_document_limit(document: &str, limit: usize) -> Result<(), QualificationError> {
    if document.len() <= limit {
        Ok(())
    } else {
        Err(QualificationError::DocumentLimit { kind: "baseline", limit })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineWire {
    schema_version: u32,
    id: String,
    profile_id: String,
    subject_revision: String,
    evidence_digest: String,
    entries: Vec<BaselineEntryWire>,
}

impl BaselineWire {
    fn validate(self) -> Result<BaselineManifest, QualificationError> {
        if self.schema_version != 1 {
            return Err(QualificationError::invalid_value(
                "baseline.schema_version",
                "only schema version 1 is supported",
            ));
        }
        let entries = self
            .entries
            .into_iter()
            .map(BaselineEntryWire::validate)
            .collect::<Result<Vec<_>, _>>()?;
        BaselineManifest::new(
            StableId::new(self.id)?,
            StableId::new(self.profile_id)?,
            self.subject_revision,
            Sha256Digest::parse(self.evidence_digest)?,
            entries,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineEntryWire {
    workload_id: String,
    metric: Metric,
    statistic: Statistic,
    value: u64,
    sample_count: usize,
}

impl BaselineEntryWire {
    fn validate(self) -> Result<BaselineEntry, QualificationError> {
        BaselineEntry::new(
            StableId::new(self.workload_id)?,
            self.metric,
            self.statistic,
            self.value,
            self.sample_count,
        )
    }
}
