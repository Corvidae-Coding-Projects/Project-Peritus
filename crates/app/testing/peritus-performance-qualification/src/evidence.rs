//! Atomic, content-addressed retention for completed H3 campaigns.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use peritus_benchmarks::{
    ArtifactPath, BaselineManifest, DatasetLimits, EvidenceArtifact, EvidenceManifest,
    EvidenceManifestBuilder, QualificationDataset, QualificationError, QualificationReport,
    Sha256Digest, baseline_from_json,
};
use serde_json::Serializer;

use crate::baseline_candidate::derive_candidate;
use crate::evidence_io::{copy_executable, write_private};
use crate::{CampaignOutcome, EvidenceError};

const PROFILE_PATH: &str = "inputs/profile.json";
const WORKLOAD_PATH: &str = "inputs/workloads.json";
const BASELINE_PATH: &str = "inputs/accepted-baseline.json";
const SUBJECT_PATH: &str = "identity/peritusd";
const RUNNER_PATH: &str = "identity/qualification-runner";
const MEASUREMENTS_PATH: &str = "results/measurements.ndjson";
const RECEIPTS_PATH: &str = "results/receipts.json";
const ACCOUNTING_PATH: &str = "results/accounting.json";
const MACHINE_PATH: &str = "results/machine.json";
const BASELINE_CANDIDATE_PATH: &str = "baseline-candidate.json";

/// Successfully published H3 evidence and its in-memory content bindings.
pub struct PublishedEvidence {
    root: PathBuf,
    manifest: EvidenceManifest,
    report: QualificationReport,
    baseline_candidate: Option<BaselineManifest>,
    baseline_candidate_digest: Option<Sha256Digest>,
}

impl PublishedEvidence {
    /// Returns the newly published evidence directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the content-addressed primary-artifact manifest.
    #[must_use]
    pub const fn manifest(&self) -> &EvidenceManifest {
        &self.manifest
    }

    /// Returns the evaluation report bound to the manifest digest.
    #[must_use]
    pub const fn report(&self) -> &QualificationReport {
        &self.report
    }

    /// Returns the fully observed derived baseline, which remains unaccepted until a later command
    /// supplies its exact document digest.
    #[must_use]
    pub const fn baseline_candidate(&self) -> Option<&BaselineManifest> {
        self.baseline_candidate.as_ref()
    }

    /// Returns the SHA-256 of the exact pretty JSON candidate file, including its final newline.
    #[must_use]
    pub const fn baseline_candidate_digest(&self) -> Option<&Sha256Digest> {
        self.baseline_candidate_digest.as_ref()
    }
}

/// Publishes one complete campaign without overwriting prior evidence.
pub struct CampaignEvidenceWriter;

impl CampaignEvidenceWriter {
    /// Validates exact source documents and binaries, then atomically publishes a private bundle.
    ///
    /// `runner_executable` must be the exact binary represented by the campaign's runner
    /// descriptor. `baseline_document` is required exactly when the campaign evaluated against an
    /// accepted baseline.
    ///
    /// # Errors
    ///
    /// Rejects document or executable identity drift, an existing output, serialization failure,
    /// and any filesystem failure. A failed publication leaves no final bundle path.
    pub fn publish(
        output: &Path,
        profile_document: &str,
        workload_document: &str,
        baseline_document: Option<&str>,
        runner_executable: &Path,
        outcome: &CampaignOutcome,
    ) -> Result<PublishedEvidence, EvidenceError> {
        validate_documents(profile_document, workload_document, baseline_document, outcome)?;
        require_new_output(output)?;
        let parent = usable_parent(output)?;
        fs::create_dir_all(parent)
            .map_err(|source| EvidenceError::io("create evidence parent", parent, source))?;
        let staging = tempfile::Builder::new().prefix(".peritus-h3-").tempdir_in(parent).map_err(
            |source| EvidenceError::io("create evidence staging directory", parent, source),
        )?;
        fs::set_permissions(staging.path(), fs::Permissions::from_mode(0o700)).map_err(
            |source| {
                EvidenceError::io("protect evidence staging directory", staging.path(), source)
            },
        )?;

        let artifacts = write_primary_artifacts(
            staging.path(),
            profile_document,
            workload_document,
            baseline_document,
            runner_executable,
            outcome,
        )?;
        let manifest = build_manifest(profile_document, workload_document, artifacts, outcome)?;
        let report = QualificationReport::new(&manifest, outcome.evaluation().clone())?;
        write_private(&staging.path().join("manifest.json"), &manifest.canonical_json()?)?;
        let mut report_bytes = report.pretty_json()?.into_bytes();
        report_bytes.push(b'\n');
        write_private(&staging.path().join("report.json"), &report_bytes)?;
        let baseline_candidate = derive_candidate(&manifest, outcome.evaluation())?;
        let baseline_candidate_digest = if let Some(candidate) = &baseline_candidate {
            let mut bytes = candidate.pretty_json()?.into_bytes();
            bytes.push(b'\n');
            write_private(&staging.path().join(BASELINE_CANDIDATE_PATH), &bytes)?;
            Some(Sha256Digest::of_bytes(&bytes))
        } else {
            None
        };

        fs::rename(staging.path(), output)
            .map_err(|source| EvidenceError::io("publish evidence bundle", output, source))?;
        Ok(PublishedEvidence {
            root: output.to_path_buf(),
            manifest,
            report,
            baseline_candidate,
            baseline_candidate_digest,
        })
    }
}

fn write_primary_artifacts(
    root: &Path,
    profile_document: &str,
    workload_document: &str,
    baseline_document: Option<&str>,
    runner_executable: &Path,
    outcome: &CampaignOutcome,
) -> Result<Vec<EvidenceArtifact>, EvidenceError> {
    let mut artifacts = vec![
        write_artifact(root, PROFILE_PATH, "application/json", profile_document.as_bytes())?,
        write_artifact(root, WORKLOAD_PATH, "application/json", workload_document.as_bytes())?,
    ];
    if let Some(document) = baseline_document {
        artifacts.push(write_artifact(
            root,
            BASELINE_PATH,
            "application/json",
            document.as_bytes(),
        )?);
    }
    artifacts.push(copy_executable(
        root,
        SUBJECT_PATH,
        "subject",
        outcome.subject_executable(),
        outcome.subject().executable_digest(),
    )?);
    artifacts.push(copy_executable(
        root,
        RUNNER_PATH,
        "runner",
        runner_executable,
        outcome.runner().implementation_digest(),
    )?);
    let measurements = measurement_lines(outcome)?;
    artifacts.push(write_artifact(root, MEASUREMENTS_PATH, "application/x-ndjson", &measurements)?);
    artifacts.push(write_json_artifact(root, RECEIPTS_PATH, outcome.receipts())?);
    artifacts.push(write_json_artifact(root, ACCOUNTING_PATH, outcome.accounting())?);
    artifacts.push(write_json_artifact(root, MACHINE_PATH, outcome.machine())?);
    Ok(artifacts)
}

fn build_manifest(
    profile_document: &str,
    workload_document: &str,
    artifacts: Vec<EvidenceArtifact>,
    outcome: &CampaignOutcome,
) -> Result<EvidenceManifest, EvidenceError> {
    let measurement_count = u64::try_from(outcome.measurements().records().len())
        .map_err(|_| QualificationError::ArithmeticOverflow("evidence measurement count"))?;
    let mut builder = EvidenceManifestBuilder::new(
        outcome.measurements().run_id().clone(),
        outcome.measurements().profile_id().clone(),
        outcome.subject().clone(),
        outcome.runner().clone(),
        outcome.machine().reference_machine().clone(),
    )
    .dataset_digests(
        Sha256Digest::of_bytes(profile_document.as_bytes()),
        Sha256Digest::of_bytes(workload_document.as_bytes()),
    )
    .time_range(outcome.started_unix_micros(), outcome.finished_unix_micros())?
    .measurement_count(measurement_count);
    for artifact in artifacts {
        builder = builder.artifact(artifact);
    }
    Ok(builder.build()?)
}

fn validate_documents(
    profile_document: &str,
    workload_document: &str,
    baseline_document: Option<&str>,
    outcome: &CampaignOutcome,
) -> Result<(), EvidenceError> {
    let dataset = QualificationDataset::from_json(
        profile_document,
        workload_document,
        DatasetLimits::production_defaults(),
    )?;
    if &dataset != outcome.dataset() {
        return Err(EvidenceError::DatasetMismatch);
    }
    let retained_baseline = baseline_document
        .map(|document| baseline_from_json(document, DatasetLimits::production_defaults()))
        .transpose()?;
    if retained_baseline.as_ref() != outcome.baseline() {
        return Err(EvidenceError::BaselineMismatch);
    }
    Ok(())
}

fn measurement_lines(outcome: &CampaignOutcome) -> Result<Vec<u8>, EvidenceError> {
    let mut bytes = Vec::new();
    for record in outcome.measurements().records() {
        let mut serializer = Serializer::new(&mut bytes);
        serde::Serialize::serialize(record, &mut serializer).map_err(|source| {
            QualificationError::Serialization { kind: "measurement record", source }
        })?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn write_json_artifact<T: serde::Serialize + ?Sized>(
    root: &Path,
    relative: &str,
    value: &T,
) -> Result<EvidenceArtifact, EvidenceError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| {
        QualificationError::Serialization { kind: "evidence artifact", source }
    })?;
    bytes.push(b'\n');
    write_artifact(root, relative, "application/json", &bytes)
}

fn write_artifact(
    root: &Path,
    relative: &str,
    media_type: &str,
    bytes: &[u8],
) -> Result<EvidenceArtifact, EvidenceError> {
    let path = ArtifactPath::new(relative)?;
    write_private(&root.join(relative), bytes)?;
    Ok(EvidenceArtifact::from_bytes(path, media_type, bytes)?)
}

fn require_new_output(output: &Path) -> Result<(), EvidenceError> {
    if output.exists() {
        Err(EvidenceError::OutputExists(output.to_path_buf()))
    } else if output.file_name().is_none() {
        Err(EvidenceError::InvalidPath(output.to_path_buf()))
    } else {
        Ok(())
    }
}

fn usable_parent(output: &Path) -> Result<&Path, EvidenceError> {
    let parent = output.parent().ok_or_else(|| EvidenceError::InvalidPath(output.to_path_buf()))?;
    if parent.as_os_str().is_empty() { Ok(Path::new(".")) } else { Ok(parent) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_refuses_an_existing_bundle() {
        let temporary = tempfile::tempdir().expect("temporary root");
        assert!(matches!(
            require_new_output(temporary.path()),
            Err(EvidenceError::OutputExists(_))
        ));
    }
}
