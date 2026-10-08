//! Immutable, idempotent native observation page publication.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read as _, Write as _},
    path::{Path, PathBuf},
};

use peritus_types::ProcessId;
use tempfile::NamedTempFile;

use super::{hex, sync_directory};
use crate::{
    ErrorCode, ProcessError, ProcessOperation, RecoveryClass,
    consumption::{store_cause, store_error},
    native::observation::{
        MAX_PAGE_BYTES, NativeObservationFrontier, StoredObservationPage,
    },
};

pub(crate) fn persist_native_observation_page(
    root: &Path,
    page: &StoredObservationPage,
) -> Result<(), ProcessError> {
    let process_directory = process_directory(root, page.process_id());
    match fs::create_dir(&process_directory) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(store_cause(
                "native observation process directory cannot be created",
                error,
            ));
        }
    }
    validate_directory(root, &process_directory)?;
    // Retrying the parent sync is required even when a prior call created the directory and then
    // reported that its root-directory synchronization failed.
    sync_directory(root)?;
    let target = page_path(&process_directory, page.ordinal());
    let bytes = page.encode()?;
    match fs::symlink_metadata(&target) {
        Ok(_) => return exact_existing(&target, &bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(store_cause(
                "native observation page cannot be inspected",
                error,
            ));
        }
    }
    let mut staging = NamedTempFile::new_in(&process_directory)
        .map_err(|error| store_cause("native observation staging file cannot be created", error))?;
    staging
        .write_all(&bytes)
        .and_then(|()| staging.as_file().sync_all())
        .map_err(|error| store_cause("native observation staging file cannot be synchronized", error))?;
    match staging.persist_noclobber(&target) {
        Ok(_) => sync_directory(&process_directory),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
            exact_existing(&target, &bytes)
        }
        Err(error) => Err(store_cause(
            "native observation page cannot be published",
            error.error,
        )),
    }
}

pub(crate) fn load_native_observation_page(
    root: &Path,
    process_id: ProcessId,
    ordinal: u64,
) -> Result<StoredObservationPage, ProcessError> {
    let process_directory = process_directory(root, process_id);
    validate_directory(root, &process_directory)?;
    let path = page_path(&process_directory, ordinal);
    let (_, bytes) = read_bounded_regular(&path, false)?;
    let page = StoredObservationPage::decode(&bytes)?;
    if page.process_id() != process_id || page.ordinal() != ordinal {
        return Err(observation_corrupt("native observation page path binding is invalid"));
    }
    Ok(page)
}

pub(crate) fn validate_native_observation_frontier(
    root: &Path,
    process_id: ProcessId,
    expected: NativeObservationFrontier,
) -> Result<(), ProcessError> {
    let ordinal = expected
        .page_count
        .checked_sub(1)
        .ok_or_else(|| observation_corrupt("native observation frontier is empty"))?;
    // Every canonical page binds its predecessor digest, prior producer-prefix digest, first
    // sequence, initial lifecycle phase, cumulative count, plan, backend, and producer. The
    // manifest retains the exact resulting frontier. Revalidating that immutable last page
    // therefore authenticates the accepted chain root without rescanning the lifetime history on
    // every authoritative store refresh; the earlier immutable pages remain the retained evidence.
    let page = load_native_observation_page(root, process_id, ordinal)?;
    if page.frontier()? != expected {
        return Err(observation_corrupt(
            "native observation history differs from its manifest frontier",
        ));
    }
    Ok(())
}

fn exact_existing(path: &Path, bytes: &[u8]) -> Result<(), ProcessError> {
    let (file, actual) = read_bounded_regular(path, true)?;
    if actual != bytes {
        return Err(observation_corrupt(
            "native observation page conflicts with its immutable identity",
        ));
    }
    file.sync_all()
        .map_err(|error| store_cause("native observation page cannot be synchronized", error))?;
    let directory = path
        .parent()
        .ok_or_else(|| store_error("native observation page has no protected parent"))?;
    sync_directory(directory)
}

fn read_bounded_regular(
    path: &Path,
    writable: bool,
) -> Result<(File, Vec<u8>), ProcessError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|error| store_cause("native observation page cannot be inspected", error))?;
    if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
        return Err(observation_corrupt("native observation page is not a regular file"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    let mut file = options
        .open(path)
        .map_err(|error| store_cause("native observation page cannot be opened", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| store_cause("native observation page metadata cannot be read", error))?;
    if !metadata.file_type().is_file()
        || metadata.len() > u64::try_from(MAX_PAGE_BYTES).unwrap_or(u64::MAX)
    {
        return Err(observation_corrupt("native observation page exceeds its canonical bound"));
    }
    let limit = u64::try_from(MAX_PAGE_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut actual = Vec::with_capacity(
        usize::try_from(metadata.len()).unwrap_or(MAX_PAGE_BYTES).min(MAX_PAGE_BYTES),
    );
    std::io::Read::by_ref(&mut file)
        .take(limit)
        .read_to_end(&mut actual)
        .map_err(|error| store_cause("native observation page cannot be read", error))?;
    if actual.len() > MAX_PAGE_BYTES {
        return Err(observation_corrupt("native observation page exceeds its canonical bound"));
    }
    Ok((file, actual))
}

const fn observation_corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}

fn process_directory(root: &Path, process_id: ProcessId) -> PathBuf {
    root.join(hex(process_id.as_bytes()))
}

fn page_path(process_directory: &Path, ordinal: u64) -> PathBuf {
    process_directory.join(format!("{ordinal:016x}.page"))
}

fn validate_directory(root: &Path, directory: &Path) -> Result<(), ProcessError> {
    let metadata = fs::symlink_metadata(directory)
        .map_err(|error| store_cause("native observation directory cannot be inspected", error))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(store_error("native observation path is not a real directory"));
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| store_cause("native observation root cannot be canonicalized", error))?;
    let canonical = fs::canonicalize(directory)
        .map_err(|error| store_cause("native observation directory cannot be canonicalized", error))?;
    if !canonical.starts_with(canonical_root) {
        return Err(store_error("native observation directory escaped its protected root"));
    }
    Ok(())
}
