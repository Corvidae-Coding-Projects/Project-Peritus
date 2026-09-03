//! Atomic exactly-once report publication with a separately prepared recovery path.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{BenchmarkError, evidence::InvocationReport};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Successful durable destination for one invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicationReceipt {
    /// Normal invocation report path.
    Primary(PathBuf),
    /// Recovery record used after primary publication failed.
    Recovery(PathBuf),
}

/// Prepared exactly-once publisher installed at admission.
pub struct AtomicPublisher {
    primary: PathBuf,
    recovery_directory: PathBuf,
    invocation_name: String,
    published: bool,
}

impl AtomicPublisher {
    /// Creates and probes both destinations before the invocation is admitted.
    pub fn prepare(
        evidence_directory: &Path,
        recovery_directory: &Path,
        invocation_name: String,
    ) -> Result<Self, BenchmarkError> {
        prepare_directory(evidence_directory, "prepare report directory")?;
        prepare_directory(recovery_directory, "prepare recovery directory")?;
        let primary = evidence_directory.join("invocation.json");
        if primary.exists() && !primary.is_file() {
            return Err(BenchmarkError::Workspace(format!(
                "report path is not a regular file: {}",
                primary.display()
            )));
        }
        Ok(Self {
            primary,
            recovery_directory: recovery_directory.to_path_buf(),
            invocation_name,
            published: false,
        })
    }

    /// Publishes one report, falling back to a recovery record if the primary path fails.
    pub fn publish(
        &mut self,
        report: &mut InvocationReport,
    ) -> Result<PublicationReceipt, BenchmarkError> {
        if self.published {
            return Err(BenchmarkError::DuplicateFinalization);
        }
        self.published = true;
        match write_atomic(&self.primary, report) {
            Ok(()) => Ok(PublicationReceipt::Primary(self.primary.clone())),
            Err(primary_error) => {
                report.success = false;
                report.disposition = "recovery_required";
                report.terminal_cause = "recovery";
                report.failure_kind = Some("report_publication".to_owned());
                report.failure = Some(primary_error.to_string());
                let recovery =
                    self.recovery_directory.join(format!("{}.recovery.json", self.invocation_name));
                write_atomic(&recovery, report).map_or_else(
                    |recovery_error| {
                        Err(BenchmarkError::ReportPublication {
                            primary: self.primary.clone(),
                            recovery: recovery.clone(),
                            primary_detail: primary_error.to_string(),
                            recovery_detail: recovery_error.to_string(),
                        })
                    },
                    |()| Ok(PublicationReceipt::Recovery(recovery.clone())),
                )
            }
        }
    }
}

fn prepare_directory(path: &Path, operation: &'static str) -> Result<(), BenchmarkError> {
    fs::create_dir_all(path).map_err(|error| BenchmarkError::filesystem(operation, path, error))?;
    let canonical =
        path.canonicalize().map_err(|error| BenchmarkError::filesystem(operation, path, error))?;
    if !canonical.is_dir() {
        return Err(BenchmarkError::Workspace(format!(
            "publication path is not a directory: {}",
            canonical.display()
        )));
    }
    let probe = canonical.join(format!(
        ".peritus-publication-probe-{}-{}",
        std::process::id(),
        TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .and_then(|file| file.sync_all())
        .map_err(|error| BenchmarkError::filesystem(operation, &probe, error))?;
    fs::remove_file(&probe)
        .map_err(|error| BenchmarkError::filesystem(operation, &probe, error))?;
    Ok(())
}

fn write_atomic(path: &Path, value: &InvocationReport) -> Result<(), BenchmarkError> {
    let parent = path.parent().ok_or_else(|| {
        BenchmarkError::Workspace(format!("report has no parent: {}", path.display()))
    })?;
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.{}.{sequence}.new",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("invocation"),
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file =
        OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|error| {
            BenchmarkError::filesystem("create report temporary", &temporary, error)
        })?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|error| BenchmarkError::filesystem("write report temporary", &temporary, error))?;
    fs::rename(&temporary, path)
        .map_err(|error| BenchmarkError::filesystem("publish report", path, error))?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| BenchmarkError::filesystem("sync report directory", parent, error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_publication_is_rejected() {
        let root = tempfile::tempdir().expect("root");
        let mut publisher = AtomicPublisher::prepare(
            &root.path().join("evidence"),
            &root.path().join("recovery"),
            "trial".to_owned(),
        )
        .expect("publisher");
        let mut report = crate::settlement::tests::fixture_report(root.path());
        publisher.publish(&mut report).expect("primary");
        assert!(matches!(
            publisher.publish(&mut report),
            Err(BenchmarkError::DuplicateFinalization)
        ));
    }

    #[test]
    fn primary_failure_publishes_recovery_and_downgrades_success() {
        let root = tempfile::tempdir().expect("root");
        let evidence = root.path().join("evidence");
        let mut publisher =
            AtomicPublisher::prepare(&evidence, &root.path().join("recovery"), "trial".to_owned())
                .expect("publisher");
        fs::create_dir(evidence.join("invocation.json")).expect("block primary path");
        let mut report = crate::settlement::tests::fixture_report(root.path());
        let receipt = publisher.publish(&mut report).expect("recovery");
        assert!(matches!(receipt, PublicationReceipt::Recovery(_)));
        assert!(!report.success);
        assert_eq!(report.disposition, "recovery_required");
    }
}
