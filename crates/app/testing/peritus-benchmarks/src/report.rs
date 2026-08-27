//! Content-bound qualification reports.

use serde::Serialize;

use crate::{
    EvidenceManifest, QualificationError, QualificationEvaluation, QualificationVerdict,
    Sha256Digest,
};

/// Reproducible report binding an evaluation to its evidence manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QualificationReport {
    schema_version: u32,
    evidence_manifest_digest: Sha256Digest,
    evaluation: QualificationEvaluation,
}

impl QualificationReport {
    /// Binds an evaluation to exact evidence manifest bytes.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when run or profile bindings disagree or the evidence
    /// manifest cannot be serialized for content addressing.
    pub fn new(
        manifest: &EvidenceManifest,
        evaluation: QualificationEvaluation,
    ) -> Result<Self, QualificationError> {
        if manifest.run_id() != evaluation.run_id() {
            return Err(QualificationError::MeasurementBinding {
                field: "report.run_id",
                expected: manifest.run_id().to_string(),
                observed: evaluation.run_id().to_string(),
            });
        }
        if manifest.profile_id() != evaluation.profile_id() {
            return Err(QualificationError::MeasurementBinding {
                field: "report.profile_id",
                expected: manifest.profile_id().to_string(),
                observed: evaluation.profile_id().to_string(),
            });
        }
        Ok(Self { schema_version: 1, evidence_manifest_digest: manifest.digest()?, evaluation })
    }

    /// Returns the digest of the exact evidence manifest.
    #[must_use]
    pub const fn evidence_manifest_digest(&self) -> &Sha256Digest {
        &self.evidence_manifest_digest
    }

    /// Returns the derived readiness verdict.
    #[must_use]
    pub const fn verdict(&self) -> QualificationVerdict {
        self.evaluation.verdict()
    }

    /// Returns the complete structured evaluation.
    #[must_use]
    pub const fn evaluation(&self) -> &QualificationEvaluation {
        &self.evaluation
    }

    /// Serializes deterministic compact JSON for content addressing.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when JSON serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        serde_json::to_vec(self).map_err(|source| QualificationError::Serialization {
            kind: "qualification report",
            source,
        })
    }

    /// Serializes human-readable JSON without changing report semantics.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when JSON serialization fails.
    pub fn pretty_json(&self) -> Result<String, QualificationError> {
        serde_json::to_string_pretty(self).map_err(|source| QualificationError::Serialization {
            kind: "qualification report",
            source,
        })
    }

    /// Returns the digest of deterministic compact report bytes.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when canonical report serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, QualificationError> {
        Ok(Sha256Digest::of_bytes(&self.canonical_json()?))
    }
}
