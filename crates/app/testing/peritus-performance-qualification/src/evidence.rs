//! Atomic, content-addressed retention for completed H3 campaigns.

use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use peritus_benchmarks::{
    ArtifactPath, DatasetLimits, EvidenceArtifact, EvidenceManifest, EvidenceManifestBuilder,
    QualificationDataset, QualificationError, QualificationReport, Sha256Digest,
    baseline_from_json,
};
use serde_json::Serializer;
use sha2::{Digest as _, Sha256};

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

/// Successfully published H3 evidence and its in-memory content bindings.
pub struct PublishedEvidence {
    root: PathBuf,
    manifest: EvidenceManifest,
    report: QualificationReport,
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

        fs::rename(staging.path(), output)
            .map_err(|source| EvidenceError::io("publish evidence bundle", output, source))?;
        Ok(PublishedEvidence { root: output.to_path_buf(), manifest, report })
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
    artifacts.push(write_json_artifact(root, MACHINE_PATH, outcome.machine().reference_machine())?);
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

fn copy_executable(
    root: &Path,
    relative: &str,
    role: &'static str,
    source: &Path,
    expected: &Sha256Digest,
) -> Result<EvidenceArtifact, EvidenceError> {
    let metadata = fs::metadata(source)
        .map_err(|error| EvidenceError::io("inspect executable evidence", source, error))?;
    if !metadata.is_file() {
        return Err(EvidenceError::InvalidPath(source.to_path_buf()));
    }
    let destination = root.join(relative);
    create_private_parent(&destination)?;
    let mut input = File::open(source)
        .map_err(|error| EvidenceError::io("open executable evidence", source, error))?;
    let mut output = File::create(&destination)
        .map_err(|error| EvidenceError::io("create executable evidence", &destination, error))?;
    output
        .set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| EvidenceError::io("protect executable evidence", &destination, error))?;
    let (length, observed) = copy_and_digest(source, &destination, &mut input, &mut output)?;
    if &observed != expected {
        return Err(EvidenceError::ExecutableDigestMismatch {
            role,
            expected: expected.to_string(),
            observed: observed.to_string(),
        });
    }
    Ok(EvidenceArtifact::new(
        ArtifactPath::new(relative)?,
        "application/octet-stream",
        length,
        observed,
    )?)
}

fn copy_and_digest(
    source: &Path,
    destination: &Path,
    input: &mut File,
    output: &mut File,
) -> Result<(u64, Sha256Digest), EvidenceError> {
    let mut hasher = Sha256::new();
    let mut length = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| EvidenceError::io("read executable evidence", source, error))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| EvidenceError::io("write executable evidence", destination, error))?;
        hasher.update(&buffer[..count]);
        length = length
            .checked_add(u64::try_from(count).unwrap_or(u64::MAX))
            .ok_or(QualificationError::ArithmeticOverflow("executable evidence length"))?;
    }
    output
        .sync_all()
        .map_err(|error| EvidenceError::io("sync executable evidence", destination, error))?;
    let digest = Sha256Digest::parse(lower_hex(&hasher.finalize()))?;
    Ok((length, digest))
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), EvidenceError> {
    create_private_parent(path)?;
    let mut file = File::create(path)
        .map_err(|error| EvidenceError::io("create evidence artifact", path, error))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| EvidenceError::io("protect evidence artifact", path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| EvidenceError::io("write evidence artifact", path, error))
}

fn create_private_parent(path: &Path) -> Result<(), EvidenceError> {
    let parent = path.parent().ok_or_else(|| EvidenceError::InvalidPath(path.to_path_buf()))?;
    fs::create_dir_all(parent)
        .map_err(|error| EvidenceError::io("create evidence directory", parent, error))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|error| EvidenceError::io("protect evidence directory", parent, error))
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
    use std::os::unix::fs::PermissionsExt as _;

    use peritus_benchmarks::Sha256Digest;

    use super::*;

    #[test]
    fn executable_copy_is_private_and_content_bound() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = temporary.path().join("runner");
        fs::write(&source, b"exact runner bytes").expect("source");
        let expected = Sha256Digest::of_bytes(b"exact runner bytes");

        let artifact =
            copy_executable(temporary.path(), "bundle/runner", "runner", &source, &expected)
                .expect("copy");

        let destination = temporary.path().join("bundle/runner");
        assert_eq!(artifact.digest(), &expected);
        assert_eq!(fs::read(&destination).expect("retained bytes"), b"exact runner bytes");
        assert_eq!(
            fs::metadata(destination).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn executable_copy_rejects_identity_drift() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = temporary.path().join("subject");
        fs::write(&source, b"observed").expect("source");
        let expected = Sha256Digest::of_bytes(b"different");

        assert!(matches!(
            copy_executable(temporary.path(), "bundle/subject", "subject", &source, &expected,),
            Err(EvidenceError::ExecutableDigestMismatch { role: "subject", .. })
        ));
    }

    #[test]
    fn publication_refuses_an_existing_bundle() {
        let temporary = tempfile::tempdir().expect("temporary root");
        assert!(matches!(
            require_new_output(temporary.path()),
            Err(EvidenceError::OutputExists(_))
        ));
    }
}
