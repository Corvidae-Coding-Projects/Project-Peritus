//! Reproducible, content-bound evidence manifests.

use serde::Serialize;

use crate::{
    ArtifactPath, QualificationError, ReferenceMachine, RunnerDescriptor, Sha256Digest, StableId,
    SubjectDescriptor,
};

/// One content-addressed file retained with qualification evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceArtifact {
    path: ArtifactPath,
    media_type: String,
    byte_length: u64,
    digest: Sha256Digest,
}

impl EvidenceArtifact {
    /// Describes in-memory evidence bytes without writing them.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the byte length cannot be represented or the media type
    /// is invalid.
    pub fn from_bytes(
        path: ArtifactPath,
        media_type: impl Into<String>,
        bytes: &[u8],
    ) -> Result<Self, QualificationError> {
        let byte_length = u64::try_from(bytes.len())
            .map_err(|_| QualificationError::ArithmeticOverflow("evidence byte length"))?;
        Self::new(path, media_type, byte_length, Sha256Digest::of_bytes(bytes))
    }

    /// Describes externally retained evidence using its verified length and digest.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the media type is empty, too long, or not visible ASCII.
    pub fn new(
        path: ArtifactPath,
        media_type: impl Into<String>,
        byte_length: u64,
        digest: Sha256Digest,
    ) -> Result<Self, QualificationError> {
        let media_type = media_type.into();
        if media_type.is_empty()
            || media_type.len() > 120
            || !media_type.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(QualificationError::invalid_value(
                "evidence_artifact.media_type",
                "must contain 1 through 120 visible ASCII bytes",
            ));
        }
        Ok(Self { path, media_type, byte_length, digest })
    }

    /// Returns the evidence-root-relative path.
    #[must_use]
    pub const fn path(&self) -> &ArtifactPath {
        &self.path
    }

    /// Returns the artifact digest.
    #[must_use]
    pub const fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    /// Returns artifact length in bytes.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
}

/// Builder for a complete evidence manifest.
pub struct EvidenceManifestBuilder {
    run_id: StableId,
    profile_id: StableId,
    subject: SubjectDescriptor,
    runner: RunnerDescriptor,
    reference_machine: ReferenceMachine,
    profile_digest: Option<Sha256Digest>,
    workload_catalog_digest: Option<Sha256Digest>,
    started_unix_micros: Option<u64>,
    finished_unix_micros: Option<u64>,
    measurement_count: u64,
    artifacts: Vec<EvidenceArtifact>,
}

impl EvidenceManifestBuilder {
    /// Starts a manifest with exact run, component, runner, and machine bindings.
    #[must_use]
    pub const fn new(
        run_id: StableId,
        profile_id: StableId,
        subject: SubjectDescriptor,
        runner: RunnerDescriptor,
        reference_machine: ReferenceMachine,
    ) -> Self {
        Self {
            run_id,
            profile_id,
            subject,
            runner,
            reference_machine,
            profile_digest: None,
            workload_catalog_digest: None,
            started_unix_micros: None,
            finished_unix_micros: None,
            measurement_count: 0,
            artifacts: Vec::new(),
        }
    }

    /// Binds exact profile and workload catalog bytes.
    #[must_use]
    pub fn dataset_digests(
        mut self,
        profile_digest: Sha256Digest,
        workload_catalog_digest: Sha256Digest,
    ) -> Self {
        self.profile_digest = Some(profile_digest);
        self.workload_catalog_digest = Some(workload_catalog_digest);
        self
    }

    /// Records wall-clock bounds supplied by the runner environment.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the finish time precedes the start time.
    pub fn time_range(
        mut self,
        started_unix_micros: u64,
        finished_unix_micros: u64,
    ) -> Result<Self, QualificationError> {
        if finished_unix_micros < started_unix_micros {
            return Err(QualificationError::invalid_value(
                "evidence_manifest.time_range",
                "finish must not precede start",
            ));
        }
        self.started_unix_micros = Some(started_unix_micros);
        self.finished_unix_micros = Some(finished_unix_micros);
        Ok(self)
    }

    /// Records the number of ingested measurement records.
    #[must_use]
    pub const fn measurement_count(mut self, measurement_count: u64) -> Self {
        self.measurement_count = measurement_count;
        self
    }

    /// Adds one content-addressed artifact.
    #[must_use]
    pub fn artifact(mut self, artifact: EvidenceArtifact) -> Self {
        self.artifacts.push(artifact);
        self
    }

    /// Validates completeness and stable artifact ordering.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when required dataset digests or timestamps are absent or
    /// two artifacts declare the same relative path.
    pub fn build(mut self) -> Result<EvidenceManifest, QualificationError> {
        let profile_digest = self.profile_digest.ok_or_else(|| {
            QualificationError::invalid_value("evidence_manifest.profile_digest", "is required")
        })?;
        let workload_catalog_digest = self.workload_catalog_digest.ok_or_else(|| {
            QualificationError::invalid_value(
                "evidence_manifest.workload_catalog_digest",
                "is required",
            )
        })?;
        let started_unix_micros = self.started_unix_micros.ok_or_else(|| {
            QualificationError::invalid_value("evidence_manifest.started", "is required")
        })?;
        let finished_unix_micros = self.finished_unix_micros.ok_or_else(|| {
            QualificationError::invalid_value("evidence_manifest.finished", "is required")
        })?;
        self.artifacts.sort_by(|left, right| left.path().cmp(right.path()));
        for pair in self.artifacts.windows(2) {
            if pair[0].path() == pair[1].path() {
                return Err(QualificationError::Duplicate {
                    kind: "evidence artifact path",
                    id: pair[0].path().as_str().to_owned(),
                });
            }
        }
        Ok(EvidenceManifest {
            schema_version: 1,
            run_id: self.run_id,
            profile_id: self.profile_id,
            subject: self.subject,
            runner: self.runner,
            reference_machine: self.reference_machine,
            profile_digest,
            workload_catalog_digest,
            started_unix_micros,
            finished_unix_micros,
            measurement_count: self.measurement_count,
            artifacts: self.artifacts,
        })
    }
}

/// Complete reproducibility and provenance manifest for one qualification run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EvidenceManifest {
    schema_version: u32,
    run_id: StableId,
    profile_id: StableId,
    subject: SubjectDescriptor,
    runner: RunnerDescriptor,
    reference_machine: ReferenceMachine,
    profile_digest: Sha256Digest,
    workload_catalog_digest: Sha256Digest,
    started_unix_micros: u64,
    finished_unix_micros: u64,
    measurement_count: u64,
    artifacts: Vec<EvidenceArtifact>,
}

impl EvidenceManifest {
    /// Returns the qualification run binding.
    #[must_use]
    pub const fn run_id(&self) -> &StableId {
        &self.run_id
    }

    /// Returns the profile binding.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile_id
    }

    /// Returns subject identity.
    #[must_use]
    pub const fn subject(&self) -> &SubjectDescriptor {
        &self.subject
    }

    /// Returns runner identity.
    #[must_use]
    pub const fn runner(&self) -> &RunnerDescriptor {
        &self.runner
    }

    /// Returns content-addressed artifacts in path order.
    #[must_use]
    pub fn artifacts(&self) -> &[EvidenceArtifact] {
        &self.artifacts
    }

    /// Serializes the manifest deterministically as compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when JSON serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        serde_json::to_vec(self).map_err(|source| QualificationError::Serialization {
            kind: "evidence manifest",
            source,
        })
    }

    /// Returns the digest of deterministic compact JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when canonical manifest serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, QualificationError> {
        Ok(Sha256Digest::of_bytes(&self.canonical_json()?))
    }
}
