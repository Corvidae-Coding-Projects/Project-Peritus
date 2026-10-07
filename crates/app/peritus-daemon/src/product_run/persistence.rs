//! Durable product-run snapshots and restart recovery.

#[cfg(test)]
use std::collections::BTreeMap;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductProviderSelection, ProductRunPhase, ProductRunSnapshot, encode_workbench_result_value,
};
use peritus_product_runner::ProductRunResume;
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::progress::RunProgress;
use super::{
    ContinuationSource, PreviewAggregate, PreviewOperationRecord, ProductRunRequest,
    RejectedFindingUpdate,
    ProductRunServiceError, RunRecord,
};
#[cfg(test)]
use super::{filesystem, invalid};
use crate::DaemonError;

mod deliverable;
mod continuation;
mod handoff;
pub(super) use handoff::{
    HandoffKind, replay_handoffs, validate_retry_handoff_coverage, write_handoff,
    write_handoff_retrying,
};
mod finding_bodies;
pub(super) use finding_bodies::{FindingBodyStore, FindingSourcePage};
mod interaction;
mod obligation;
pub(super) use obligation::{
    LegacyAssessment, ObligationFirstCause, ObligationRoot, PersistedObligation,
    SettlementObligation,
};
mod preview;
mod progress;
use preview::restore_preview;
mod settlement;
mod workbench;
pub(super) use workbench::load_workbench_records;

use settlement::{PersistedCheckpoint, restore_settlement};
use continuation::PersistedResumeRoot;

mod types;
use types::{
    PersistedContinuationSource, PersistedDeliverable, PersistedPreviewOperation,
    PersistedPreviewOutput, PersistedProgress, PersistedRecord, PersistedRejectedFindingUpdate, PersistedResourceCause,
    PersistedResourceCoverage, PersistedResourceIoKind, PersistedResourceMeasurement,
    PersistedResourceOperation, PersistedResourceTelemetry,
};

pub(super) const CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION: u16 = 1;

/// Durable continuation bytes that this daemon cannot safely interpret yet.
///
/// Rewriting an otherwise valid run record preserves this authority exactly. It never authorizes
/// execution until a compatible decoder validates the candidate binding.
#[derive(Clone)]
pub(super) enum OpaqueResume {
    Inline(Vec<u8>),
    Root(PersistedResumeRoot),
}

#[cfg(test)]
mod fault;
#[cfg(test)]
use fault::check_persistence_fault;
#[cfg(test)]
pub(super) use fault::{
    PersistenceFaultPoint, clear_persistent_persistence_fault, inject_persistence_fault,
    inject_persistent_persistence_fault,
};

#[cfg(test)]
pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let result = write_record(directory, record);
    if let Err(error) = &result {
        record.interaction.record_persistence_failure(error.describe());
        record.cancelled.store(true, std::sync::atomic::Ordering::Release);
        record.control_cancellation.cancel();
        let _ = record.provider_cancellation.cancel();
    }
    result
}

/// One deep, immutable capture of a canonical mutation candidate.
///
/// `RunRecord` contains live shared state. The lineage digest and encoded candidate must therefore
/// be derived from this owned DTO rather than by independently recapturing the live record.
pub(super) struct CanonicalRecordCapture {
    persisted: PersistedRecord,
}

impl CanonicalRecordCapture {
    pub(super) fn capture(record: &RunRecord) -> Result<Self, ProductRunServiceError> {
        Ok(Self { persisted: PersistedRecord::from_record(record, None)? })
    }

    pub(super) fn state_digest(
        &self,
    ) -> Result<peritus_types::Sha256Digest, ProductRunServiceError> {
        let mut value = serde_json::to_value(&self.persisted).map_err(|error| {
            ProductRunServiceError::persistence("capture product-run mutation state", error)
        })?;
    let object = value.as_object_mut().ok_or_else(|| {
        ProductRunServiceError::internal(
            "capture product-run mutation state",
            "the canonical record did not encode as an object",
        )
    })?;
    object.remove("record_revision");
    object.remove("record_lineage_root");
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        ProductRunServiceError::persistence("encode product-run mutation state", error)
    })?;
        Ok(peritus_codec::sha256(&bytes))
    }

    /// Seals the publication envelope into the same captured DTO used for the state digest.
    pub(super) fn encode(
        mut self,
        revision: u64,
        lineage_root: peritus_types::Sha256Digest,
    ) -> Result<Vec<u8>, ProductRunServiceError> {
        if revision == 0 || lineage_root == peritus_types::Sha256Digest::new([0; 32]) {
            return Err(ProductRunServiceError::InvalidState);
        }
        self.persisted.record_revision = revision;
        self.persisted.record_lineage_root = lineage_root.into_bytes();
        serde_json::to_vec_pretty(&self.persisted).map_err(|error| {
            ProductRunServiceError::persistence("encode canonical product-run candidate", error)
        })
    }
}

pub(super) fn canonical_record_digest(
    directory: &Path,
    run: RunId,
) -> Result<peritus_types::Sha256Digest, ProductRunServiceError> {
    let records = record_directory(directory)?;
    let canonical = records.join(format!("{}.json", hex(run.as_bytes())));
    match fs::read(canonical) {
        Ok(bytes) => Ok(peritus_codec::sha256(&bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(peritus_types::Sha256Digest::new([0; 32]))
        }
        Err(error) => Err(ProductRunServiceError::persistence(
            "read canonical product-run digest",
            error,
        )),
    }
}

/// Installs one fully synced candidate only over its exact durable predecessor.
pub(super) fn install_canonical_candidate(
    directory: &Path,
    run: RunId,
    attempt: u64,
    revision: u64,
    lineage_root: peritus_types::Sha256Digest,
    expected_revision: u64,
    expected_lineage_root: peritus_types::Sha256Digest,
    expected_digest: peritus_types::Sha256Digest,
    bytes: &[u8],
) -> Result<peritus_types::Sha256Digest, ProductRunServiceError> {
    use std::io::Write as _;
    let records = record_directory(directory)?;
    fs::create_dir_all(&records).map_err(|error| {
        ProductRunServiceError::persistence("create canonical product-run directory", error)
    })?;
    let canonical = records.join(format!("{}.json", hex(run.as_bytes())));
    let digest = peritus_codec::sha256(bytes);
    let candidate: PersistedRecord = serde_json::from_slice(bytes).map_err(|error| {
        ProductRunServiceError::persistence("decode canonical product-run candidate", error)
    })?;
    if candidate.publication_head(run, digest)? != (revision, lineage_root) {
        return Err(ProductRunServiceError::internal(
            "validate canonical product-run candidate",
            "the candidate publication envelope differs from its immutable lineage",
        ));
    }
    match fs::read(&canonical) {
        Ok(current) if peritus_codec::sha256(&current) == digest && current == bytes => {
            continuation::sync_directory(&records, "sync canonical product-run directory")?;
            return Ok(digest);
        }
        Ok(current) => {
            let current_digest = peritus_codec::sha256(&current);
            let predecessor: PersistedRecord = serde_json::from_slice(&current).map_err(|error| {
                ProductRunServiceError::persistence(
                    "decode canonical product-run predecessor",
                    error,
                )
            })?;
            if current_digest != expected_digest
                || predecessor.publication_head(run, current_digest)?
                    != (expected_revision, expected_lineage_root)
            {
                return Err(ProductRunServiceError::internal(
                    "install canonical product-run candidate",
                    "the durable predecessor head changed outside its joined publisher",
                ));
            }
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && expected_revision == 0
                && expected_lineage_root == peritus_types::Sha256Digest::new([0; 32])
                && expected_digest == peritus_types::Sha256Digest::new([0; 32]) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProductRunServiceError::internal(
                "install canonical product-run candidate",
                "the expected durable predecessor is missing",
            ));
        }
        Err(error) => {
            return Err(ProductRunServiceError::persistence(
                "read canonical product-run predecessor",
                error,
            ));
        }
    }
    let stage = records.join(format!(
        ".{}.{}.{}.{}.stage",
        hex(run.as_bytes()),
        attempt,
        revision,
        hex_digest(lineage_root),
    ));
    match fs::OpenOptions::new().write(true).create_new(true).open(&stage) {
        Ok(mut file) => {
            file.write_all(bytes).map_err(|error| {
                ProductRunServiceError::persistence(
                    "write canonical product-run stage",
                    error,
                )
            })?;
            file.sync_all().map_err(|error| {
                ProductRunServiceError::persistence(
                    "sync canonical product-run stage",
                    error,
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let staged = fs::read(&stage).map_err(|error| {
                ProductRunServiceError::persistence(
                    "read existing canonical product-run stage",
                    error,
                )
            })?;
            if staged != bytes {
                return Err(ProductRunServiceError::internal(
                    "validate canonical product-run stage",
                    "the unique mutation lineage stage contains different bytes",
                ));
            }
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&stage)
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    ProductRunServiceError::persistence(
                        "sync existing canonical product-run stage",
                        error,
                    )
                })?;
        }
        Err(error) => {
            return Err(ProductRunServiceError::persistence(
                "create canonical product-run stage",
                error,
            ));
        }
    }
    continuation::sync_directory(&records, "sync canonical product-run stage directory")?;
    fs::rename(&stage, &canonical).map_err(|error| {
        ProductRunServiceError::persistence("publish canonical product-run candidate", error)
    })?;
    continuation::sync_directory(&records, "sync canonical product-run directory")?;
    let workbench = records.parent().unwrap_or(records.as_path());
    continuation::sync_directory(workbench, "sync canonical workbench root")?;
    if let Some(parent) = workbench.parent().filter(|path| !path.as_os_str().is_empty()) {
        continuation::sync_directory(parent, "sync canonical workbench parent")?;
    }
    Ok(digest)
}

fn hex_digest(digest: peritus_types::Sha256Digest) -> String {
    digest.into_bytes().iter().fold(
        String::with_capacity(64),
        |mut text, byte| {
            use core::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        },
    )
}

#[cfg(test)]
pub(super) fn write_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    use std::io::Write as _;
    #[cfg(test)]
    let fault_directory = directory;
    let workbench_directory = record_directory(directory)?;
    fs::create_dir_all(&workbench_directory).map_err(|error| {
        ProductRunServiceError::persistence("create the workbench run directory", error)
    })?;
    let directory = workbench_directory.as_path();
    let workbench_root = if directory.file_name().is_some_and(|name| name == "runs")
        && directory
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "workbench-v1")
    {
        directory.parent().expect("a workbench runs directory has a parent")
    } else {
        directory
    };
    let resume_root = record
        .resume
        .as_ref()
        .map(|resume| continuation::publish(workbench_root, resume))
        .transpose()?;
    let persisted = PersistedRecord::from_record(record, resume_root)?;
    let path = directory.join(format!("{}.json", persisted.run_id));
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary).map_err(|error| {
        ProductRunServiceError::persistence("create the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeWrite,
    )?;
    // One physical I/O buffer never limits the logical retained record. Stream the existing
    // JSON layout instead of allocating a second, arbitrarily capped complete encoding.
    {
        let mut writer = std::io::BufWriter::with_capacity(32 * 1024, &mut file);
        serde_json::to_writer_pretty(&mut writer, &persisted).map_err(|error| {
            ProductRunServiceError::persistence("write the product-run temporary record", error)
        })?;
        writer.flush().map_err(|error| {
            ProductRunServiceError::persistence("flush the product-run temporary record", error)
        })?;
    }
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeFileSync,
    )?;
    file.sync_all().map_err(|error| {
        ProductRunServiceError::persistence("sync the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeRename,
    )?;
    fs::rename(temporary, path).map_err(|error| {
        ProductRunServiceError::persistence("replace the durable product-run record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::AfterRename,
    )?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeDirectorySync,
    )?;
    continuation::sync_directory(directory, "sync the product-run directory")?;
    continuation::sync_directory(workbench_root, "sync the workbench root")?;
    if let Some(parent) = workbench_root.parent().filter(|path| !path.as_os_str().is_empty()) {
        continuation::sync_directory(parent, "sync the workbench parent")?;
    }
    Ok(())
}

pub(super) fn record_directory(directory: &Path) -> Result<PathBuf, ProductRunServiceError> {
    if directory.file_name().is_some_and(|name| name == "product-runs") {
        return Ok(directory
            .parent()
            .ok_or_else(|| {
                ProductRunServiceError::internal(
                    "resolve the workbench run directory",
                    "the configured product-run directory has no parent",
                )
            })?
            .join("workbench-v1/runs"));
    }
    Ok(directory.to_path_buf())
}

#[cfg(test)]
pub(super) fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let root = directory
        .parent()
        .filter(|_| directory.file_name().is_some_and(|name| name == "product-runs"))
        .map_or_else(
            || directory.parent().unwrap_or(directory).to_path_buf(),
            |parent| parent.join("workbench-v1"),
        );
    let controls = crate::product_control::ControlStore::open(
        &root,
        peritus_journal::StoreId::new([0x7f; 16]).map_err(|_| invalid("invalid test store"))?,
    )
    .map_err(|error| {
        DaemonError::with_source(
            crate::DaemonErrorCode::Storage,
            crate::DaemonRecovery::Reconcile,
            "open governed test state",
            "the durable workbench control store could not be reopened",
            error,
        )
    })?;
    let finding_bodies = FindingBodyStore::open(&root.join("finding-bodies")).map_err(|error| {
        DaemonError::with_source(
            crate::DaemonErrorCode::Storage,
            crate::DaemonRecovery::Reconcile,
            "open test product finding bodies",
            "the durable product finding body store could not be reopened",
            error,
        )
    })?;
    load_workbench_records(&root, Some(&controls), &finding_bodies)
}

#[cfg(test)]
pub(super) fn load_unchecked_records(
    directory: &Path,
) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let workbench_directory = record_directory(directory).map_err(|error| {
        DaemonError::new(
            crate::DaemonErrorCode::Storage,
            crate::DaemonRecovery::Reconcile,
            "resolve product-run state",
            error.describe(),
        )
    })?;
    let directory = workbench_directory.as_path();
    let mut records = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(filesystem)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report_isolation_failure(directory, "enumerate a product-run record", &error);
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                quarantine_record(&path, "product-run record cannot be read", Some(&error));
                continue;
            }
        };
        let persisted: PersistedRecord = match serde_json::from_slice(&bytes) {
            Ok(persisted) => persisted,
            Err(error) => {
                quarantine_record(&path, "product-run state is malformed", Some(&error));
                continue;
            }
        };
        let record = match persisted
            .into_record_with_context(Some("test-only persisted workbench context"))
        {
            Ok(record) => record,
            Err(error) => {
                quarantine_record(&path, "product-run state contains invalid values", Some(&error));
                continue;
            }
        };
        if !record_path_matches(&path, record.request.run_id()) {
            quarantine_record(
                &path,
                "product-run filename does not match its embedded identity",
                None,
            );
            continue;
        }
        if records.contains_key(&record.request.run_id()) {
            quarantine_record(&path, "duplicate product-run identity", None);
            continue;
        }
        records.insert(record.request.run_id(), record);
    }
    Ok(records)
}

pub(super) fn quarantine_record(path: &Path, reason: &str, source: Option<&dyn std::fmt::Display>) {
    if path.parent().and_then(Path::file_name).is_some_and(|name| name == ".quarantine") {
        return;
    }
    isolate_record(path, ".quarantine", reason, source);
}

pub(super) fn retire_record(path: &Path, reason: &str) {
    isolate_record(path, ".retired", reason, None);
}

fn isolate_record(
    path: &Path,
    directory_name: &str,
    reason: &str,
    source: Option<&dyn std::fmt::Display>,
) {
    let Some(parent) = path.parent() else {
        report_isolation(path, reason, source, None);
        return;
    };
    let isolation_directory = parent.join(directory_name);
    let result = fs::create_dir_all(&isolation_directory).and_then(|()| {
        let destination = available_isolation_path(&isolation_directory, path);
        fs::rename(path, destination)
    });
    report_isolation(path, reason, source, result.err().as_ref());
}

fn available_isolation_path(directory: &Path, source: &Path) -> PathBuf {
    let name = source.file_name().and_then(|value| value.to_str()).unwrap_or("run.json");
    let initial = directory.join(name);
    if !initial.exists() {
        return initial;
    }
    for suffix in 1_u32.. {
        let candidate = directory.join(format!("{name}.{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 isolation suffixes are exhaustive")
}

fn report_isolation(
    path: &Path,
    reason: &str,
    source: Option<&dyn std::fmt::Display>,
    isolation_error: Option<&std::io::Error>,
) {
    let source = source.map_or(String::new(), |error| format!(": {error}"));
    let isolation = isolation_error
        .map_or(String::new(), |error| format!("; could not move the file aside: {error}"));
    crate::diagnostic::report(&format!(
        "peritusd: isolated unusable run projection {}: {reason}{source}{isolation}",
        path.display()
    ));
}

fn report_isolation_failure(path: &Path, operation: &str, error: &std::io::Error) {
    crate::diagnostic::report(&format!(
        "peritusd: skipped {} while attempting to {operation}: {error}",
        path.display()
    ));
}

fn record_path_matches(path: &Path, run_id: RunId) -> bool {
    path.file_name().and_then(|name| name.to_str())
        == Some(format!("{}.json", hex(run_id.as_bytes())).as_str())
}

mod record;
use record::hex;

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
